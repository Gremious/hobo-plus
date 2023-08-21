use hobo::{prelude::*, signal::SignalExt};
use super::{window, closure_mut};
use super::entity_ext::AsEntityExt;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct FontTag;

/// Allows you to tell whether it is currently being clicked on (mousedown active).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Clicked(pub bool);

pub struct ChildrenDiffConfig<K, V, E, Insert, OnChange, OnRemove, OnUpdate> {
	insert: Insert,
	on_change: OnChange,
	on_remove: OnRemove,
	on_update: OnUpdate,
	_pd: std::marker::PhantomData<(K, V, E)>,
}

pub struct ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, OnRemove, OnUpdate> {
	insert: Option<Insert>,
	on_change: OnChange,
	on_remove: OnRemove,
	on_update: OnUpdate,
	_pd: std::marker::PhantomData<(K, V, E)>,
}

impl<E, Insert> ChildrenDiffConfig<(), (), E, Insert, fn(), fn(&()), fn(&(), &hobo::signal::Mutable<()>)> {
	pub fn builder<K, V>() -> ChildrenDiffConfigBuilder<K, V, E, Insert, fn(), fn(&K), fn(&K, &hobo::signal::Mutable<V>)> { ChildrenDiffConfigBuilder {
		insert: None,
		on_change: move || {},
		on_remove: move |_| {},
		on_update: move |_, _| {},
		_pd: std::marker::PhantomData,
	} }
}

impl<K, V, E, Insert, OnChange, OnRemove, OnUpdate> ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, OnRemove, OnUpdate> where
	E: hobo::AsElement + 'static,
	Insert: FnMut(&K, &hobo::signal::Mutable<V>) -> E + 'static,
	OnChange: FnMut() + 'static,
	OnRemove: FnMut(&K) + 'static,
	OnUpdate: FnMut(&K, &hobo::signal::Mutable<V>) + 'static,
{
	pub fn insert(mut self, f: Insert) -> Self { self.insert = Some(f); self }
	pub fn on_change<NewOnChange>(self, f: NewOnChange) -> ChildrenDiffConfigBuilder<K, V, E, Insert, NewOnChange, OnRemove, OnUpdate> where
		NewOnChange: FnMut() + 'static,
	{ ChildrenDiffConfigBuilder {
		insert: self.insert,
		on_change: f,
		on_remove: self.on_remove,
		on_update: self.on_update,
		_pd: std::marker::PhantomData,
	} }
	pub fn on_remove<NewOnRemove>(self, f: NewOnRemove) -> ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, NewOnRemove, OnUpdate> where
		NewOnRemove: FnMut(&K) + 'static,
	{ ChildrenDiffConfigBuilder {
		insert: self.insert,
		on_change: self.on_change,
		on_remove: f,
		on_update: self.on_update,
		_pd: std::marker::PhantomData,
	} }
	pub fn on_update<NewOnUpdate>(self, f: NewOnUpdate) -> ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, OnRemove, NewOnUpdate> where
		NewOnUpdate: FnMut(&K, &hobo::signal::Mutable<V>) + 'static,
	{ ChildrenDiffConfigBuilder {
		insert: self.insert,
		on_change: self.on_change,
		on_remove: self.on_remove,
		on_update: f,
		_pd: std::marker::PhantomData,
	} }

	pub fn build(self) -> ChildrenDiffConfig<K, V, E, Insert, OnChange, OnRemove, OnUpdate> {
		ChildrenDiffConfig {
			insert: self.insert.unwrap(),
			on_change: self.on_change,
			on_remove: self.on_remove,
			on_update: self.on_update,
			_pd: std::marker::PhantomData,
		}
	}
}

pub struct ChildrenDiff<K, V> where
	K: Ord + Clone + std::hash::Hash + 'static,
	V: 'static,
{
	/// Mutable which is being updated/watched.
	pub mutable: hobo::signal_map::MutableBTreeMap<K, hobo::signal::Mutable<V>>,
	/// Element which gets items appended/removed.
	pub element: hobo::Element,
	/// Hobo elements that represent the current state.
	pub items: std::collections::BTreeMap<K, hobo::Element>,
	/// "kind of a hack to avoid running on_change too often"
	unprocessed_ids: std::collections::HashSet<K>,
}

impl<K, V> ChildrenDiff<K, V> where
	K: Ord + Clone + std::hash::Hash + 'static,
	V: 'static,
{
	pub fn add(&mut self, key: K, value: V) {
		let mut mutable_lock = self.mutable.lock_mut();
		if mutable_lock.insert_cloned(key.clone(), hobo::signal::Mutable::new(value)).is_some() {
			log::warn!("ChildrenDiff::add overriding existing value, this is likely an error");
		}
		self.unprocessed_ids.insert(key);
	}

	pub fn update(&mut self, key: K, value: V) {
		let mut mutable_lock = self.mutable.lock_mut();
		let value_mutable = mutable_lock.get(&key).unwrap().clone();
		value_mutable.set(value);
		// this is to trigger MapDiff::Update
		mutable_lock.insert_cloned(key.clone(), value_mutable);
		self.unprocessed_ids.insert(key);
	}

	pub fn update_with(&mut self, key: K, f: impl FnOnce(&mut V)) {
		let mut mutable_lock = self.mutable.lock_mut();
		let value_mutable = mutable_lock.get(&key).unwrap().clone();
		f(&mut value_mutable.lock_mut());
		// this is to trigger MapDiff::Update
		mutable_lock.insert_cloned(key.clone(), value_mutable);
		self.unprocessed_ids.insert(key);
	}

	pub fn remove(&mut self, key: K) {
		let mut mutable_lock = self.mutable.lock_mut();
		mutable_lock.remove(&key);
		self.unprocessed_ids.insert(key);
	}
}

pub trait AsElementExt: AsElement {
	fn children_diff<K, V, E, Insert, OnChange, OnRemove, OnUpdate>(self, config: ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, OnRemove, OnUpdate>) -> Self where
		Self: Sized + Copy + 'static,
		K: Ord + Clone + std::hash::Hash + 'static,
		V: 'static,
		E: hobo::AsElement + 'static,
		Insert: FnMut(&K, &hobo::signal::Mutable<V>) -> E + 'static,
		OnChange: FnMut() + 'static,
		OnRemove: FnMut(&K) + 'static,
		OnUpdate: FnMut(&K, &hobo::signal::Mutable<V>) + 'static,
	{
		use hobo::{signal_map::{MapDiff, MutableBTreeMap}, signal::Mutable};

		let ChildrenDiffConfig { mut insert, mut on_change, mut on_remove, mut on_update, .. } = config.build();
		let mutable = MutableBTreeMap::<K, Mutable<V>>::new();
		self
			.component(mutable.signal_map_cloned().subscribe(move |diff| match diff {
				MapDiff::Insert { key, value } => {
					{
						let element = insert(&key, &value).as_element();
						self.add_child(element);

						let mut children_diff = self.get_cmp_mut::<ChildrenDiff<K, V>>();
						children_diff.unprocessed_ids.remove(&key);
						children_diff.items.insert(key, element);
						if !children_diff.unprocessed_ids.is_empty() { return; }
					}

					on_change();
				},
				MapDiff::Remove { key } => {
					{
						let element = self.get_cmp_mut::<ChildrenDiff<K, V>>().items.remove(&key).unwrap();
						element.remove();
						on_remove(&key);

						let mut children_diff = self.get_cmp_mut::<ChildrenDiff<K, V>>();
						children_diff.unprocessed_ids.remove(&key);
						if !children_diff.unprocessed_ids.is_empty() { return; }
					}

					on_change();
				},
				MapDiff::Update { key, value } => {
					{
						on_update(&key, &value);

						let mut children_diff = self.get_cmp_mut::<ChildrenDiff<K, V>>();
						children_diff.unprocessed_ids.remove(&key);
						if !children_diff.unprocessed_ids.is_empty() { return; }
					}

					on_change();
				},
				MapDiff::Replace { .. } | MapDiff::Clear { } => unimplemented!(),
			}))
			.component(ChildrenDiff { mutable, element: self.as_element(), items: Default::default(), unprocessed_ids: Default::default() })
	}

	/// Adds an `data-name` attribute to the element with a value of T
	fn name_typed<T: 'static>(self) -> Self {
		if self.is_dead() { log::warn!("mark dead {:?}", self.as_entity()); return self; }
		let name = core::any::type_name::<T>();
		let name = name.rsplit_once(':').map_or(name, |s| s.1);
		self.attr("data-name", name)
	}

	fn mark_and_name<T: 'static>(self) -> Self { self.mark::<T>().name_typed::<T>() }

	/// Adds the `Clicked` component to an element which allows you to tell whether it is currently being clicked on (mousedown active).
	///
	/// Uses the default window (e.g. [web_sys::window()]).
	///
	/// See: `clicked()`.
	fn report_clicked(self) -> Self where Self: Sized + Copy + 'static {
		self.report_clicked_on_window(window())
	}

	/// Adds the `Clicked` component to an element which allows you to tell whether it is currently being clicked on (mousedown active).
	///
	/// Uses the passed in [web_sys::Window].
	///
	/// See: `clicked()`.
	fn report_clicked_on_window(self, window: web_sys::Window) -> Self where Self: Sized + Copy + 'static {
		if self.try_get_cmp::<Clicked>().is_some() {
			return self;
		} else {
			self.add_component(Clicked(false));
			self.add_on_mouse_down(move |e| { e.prevent_default(); self.get_cmp_mut::<Clicked>().0 = true; });
			self.bundle(window.on_mouse_up(move |_| self.get_cmp_mut::<Clicked>().0 = false));
		}
		self
	}

	/// This will panic at runtime if the `Clicked` component is not present.
	/// Make sure to actually call report_clicked() on the element first.
	fn clicked(&self) -> bool { self.try_get_cmp::<Clicked>().and_then(|x| Some(x.0)).unwrap_or(false) }

	fn font(self, style: &css::Style) -> Self { self.class_typed::<FontTag>(style.clone()) }

	// client_rect.width()/.height() are with padding + border
	// use client_width() for with padding but no borders/margins/etc
	fn width(&self) -> f64 {
		let element_rect = self.get_cmp::<web_sys::Element>().get_bounding_client_rect();
		element_rect.right() - element_rect.left()
	}

	fn height(&self) -> f64 {
		let element_rect = self.get_cmp::<web_sys::Element>().get_bounding_client_rect();
		element_rect.bottom() - element_rect.top()
	}

	#[inline] fn top(&self) -> f64 { self.get_cmp::<web_sys::Element>().get_bounding_client_rect().top() }
	#[inline] fn right(&self) -> f64 { self.get_cmp::<web_sys::Element>().get_bounding_client_rect().right() }
	#[inline] fn bottom(&self) -> f64 { self.get_cmp::<web_sys::Element>().get_bounding_client_rect().bottom() }
	#[inline] fn left(&self) -> f64 { self.get_cmp::<web_sys::Element>().get_bounding_client_rect().left() }

	/// Auto-flips an element if it would be off-screen, by mirroring the top/bottom/left/right positional properties appropriately.
	///
	/// This also counts as setting the prefered position for the element, so you do not need to add it in a class/style yourself.
	///
	/// # Arguments
	///
	/// * `spacing_v` - A top or bottom property with the amount of spacing between the parent and child e.g. Some(css::top!(8 px))
	/// * `spacing_h` - A left or right property with the amount of spacing between the parent and child e.g. Some(css::right!(36 px))
	///
	/// Note that it is not e.g. "100% + 8 px", but only the "margin".
	///
	/// Currently only px units are supported.
	fn flip_if_offscreen(self, spacing_v: Option<css::Property>, spacing_h: Option<css::Property>) {
		let parent = self.parent();
		let self_height = self.height();
		let self_width = self.width();
		let window_height = window().inner_height().unwrap().as_f64().unwrap();
		let window_width = window().inner_width().unwrap().as_f64().unwrap();
		let mut new_style = Vec::new();

		if let Some(v) = spacing_v {
			if let css::Property::Top(css::PositionOffset::Some(css::Unit::Px(f))) = v {
				let vertical = f.into_inner() as f64;
				let dimension = css::PositionOffset::Some(css::unit!(100% + vertical px));
				let property = if parent.bottom() + vertical + self_height > window_height {
					css::Property::Bottom(dimension)
				} else {
					css::Property::Top(dimension)
				};
				new_style.push(property);
			} else if let css::Property::Bottom(css::PositionOffset::Some(css::Unit::Px(f))) = v {
				let vertical = f.into_inner() as f64;
				let dimension = css::PositionOffset::Some(css::unit!(100% + vertical px));
				let property = if parent.top() - vertical - self_height < 0. {
					css::Property::Top(dimension)
				} else {
					css::Property::Bottom(dimension)
				};
				new_style.push(property);
			} else {
				log::warn!("Flip on element with a non-pixel position! (or not top/bottom?)")
			}
		}

		if let Some(h) = spacing_h {
			if let css::Property::Left(css::PositionOffset::Some(css::Unit::Px(f))) = h {
				let horizontal = f.into_inner() as f64;
				let dimension = css::PositionOffset::Some(css::unit!(100% - horizontal px));
				let property = if parent.right() + horizontal + self_width > window_width {
					css::Property::Right(dimension)
				} else {
					css::Property::Left(dimension)
				};
				new_style.push(property);
			} else if let css::Property::Right(css::PositionOffset::Some(css::Unit::Px(f))) = h {
				let horizontal = f.into_inner() as f64;
				let dimension = css::PositionOffset::Some(css::unit!(100% - horizontal px));
				let property = if parent.left() - horizontal - self_width < 0. {
					css::Property::Left(dimension)
				} else {
					css::Property::Right(dimension)
				};
				new_style.push(property);
			} else {
				log::warn!("Flip on element with a non-pixel position! (or not left/right?)")
			}
		}

		self.set_style(new_style);
	}

	fn hide_signal(self, signal: impl hobo::signal::Signal<Item=bool> + 'static) -> Self where Self: 'static {
		struct HideSignalStyleTag;
		self.class_typed_signal::<HideSignalStyleTag, _, _>(signal.map(move |x| if x { css::properties![css::display!(none)] } else { css::properties![] }))
	}

	/// The chaining counterpart of [set_on_slide](Self::set_on_slide).
	fn on_slide(self, f: impl FnMut(f64) + 'static) -> Self where Self: Sized + Copy + 'static { self.add_on_slide(f); self }

	/// Provides a closure which triggers on mouse move, only while the element is clicked.
	/// It captures a normalized `f64` which indicates where the mouse currently is on the element (left-right).
	///
	/// This is a non-chaining function. For the chaining counterpart, see [on_slide](Self::on_slide).
	fn add_on_slide(self, mut f: impl FnMut(f64) + 'static) where Self: Sized + Copy + 'static {
		self
			.report_clicked()
			.add_bundle(window().on_mouse_move(move |mouse_event: web_sys::MouseEvent| {
				if !self.clicked() { return; }
				let mouse_x = mouse_event.client_x() as f64;
				let position = f64::clamp((mouse_x - self.left()) / self.width(), 0.0, 1.0);
				f(position);
			}));
	}

	fn with_on_slide(self, mut f: impl FnMut(&Self, f64) + 'static) -> Self where Self: Sized + Copy + 'static {
		self.on_slide(move |e| f(&self, e))
	}

	/// The chaining counterpart of [set_on_first_flow](Self::set_on_first_flow).
	fn on_next_flow(self, f: impl FnOnce() + 'static) -> Self where Self: Sized + Copy + 'static {
		self.set_on_next_flow(f); self
	}

	/// Provides a closure which triggers once, after the next reflow completes.
	///
	/// In practice, when creating an element with `.on_next_flow(|| ... )`,
	/// it will trigger immediately after the page's first flow.
	///
	/// However, if used in conjunction with a function that is called multiple times, e.g.
	/// ```ignore
	///		window().on_resize(move |_| element.set_on_next_flow(|| /* ... */ ))
	/// ```
	/// it will re-trigger after each reflow.
	///
	/// This is a non-chaining function. For the chaining counterpart, see [on_first_flow](Self::on_first_flow).
	fn set_on_next_flow(self, f: impl FnOnce() + 'static) where Self: Sized + Copy + 'static {
		window().request_animation_frame(Closure::once_into_js(f).unchecked_ref()).unwrap();
	}

	/// The chaining counterpart of [set_on_intersection](Self::set_on_intersection).
	fn on_intersection(self, f: impl FnMut(Vec<web_sys::IntersectionObserverEntry>) + 'static) -> Self where Self: Copy + 'static {
		self.set_on_intersection(f);
		self
	}

	/// Boilerplate for using the [IntersectionObserverAPI](https://developer.mozilla.org/en-US/docs/Web/API/Intersection_Observer_API).
	///
	/// Creates a new observer with the passed in parameters,
	/// saves the closure and the observer as a component,
	/// and then immediately calls observe on the element,
	///
	/// This is a non-chaining function. For the chaining counterpart, see [on_intersection](Self::on_intersection).
	fn set_on_intersection(self, f: impl FnMut(Vec<web_sys::IntersectionObserverEntry>) + 'static) {
		let closure = closure_mut(f);

		let observer = web_sys::IntersectionObserver::new_with_options(closure.as_ref().unchecked_ref(), &web_sys::IntersectionObserverInit::new()).unwrap();
		observer.observe(&self.get_cmp::<web_sys::Element>());

		self.add_component(closure);
		self.add_component(observer);
	}
}

impl<T: AsElement> AsElementExt for T {}
