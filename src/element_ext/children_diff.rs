#![expect(clippy::type_complexity)]

use hobo::prelude::*;
use hobo::{signal_map::{MapDiff, MutableBTreeMap, SignalMapExt}, signal::SignalExt};

// TODO: rename?
#[derive(Clone)]
pub struct SeriousValue<K, V> where
	K: Ord + Clone + std::hash::Hash + 'static,
	V: Clone + 'static,
{
	element: hobo::Element,
	key: K,
	initial_value: V,
	broadcaster: hobo::signal::Broadcaster<hobo::signal_map::MapWatchKeySignal<hobo::signal_map::MutableSignalMap<K, V>>>,
	_pd: std::marker::PhantomData<(K, V)>,
}

impl<K, V> SeriousValue<K, V> where
	K: Ord + Clone + std::hash::Hash + 'static,
	V: Clone + 'static,
{
	fn new(element: hobo::Element, key: K, initial_value: V) -> Self {
		let broadcaster = element.get_cmp::<ChildrenDiff<K, V>>().mutable.signal_map_cloned().key_cloned(key.clone()).broadcast();
		Self { element, key, initial_value, broadcaster, _pd: std::marker::PhantomData }
	}

	pub fn current(&self) -> V {
		self.element.get_cmp::<ChildrenDiff<K, V>>().mutable.lock_ref().get(&self.key).unwrap().clone()
	}

	pub fn map_ref<R>(&self, f: impl FnOnce(&V) -> R) -> R {
		f(self.element.get_cmp::<ChildrenDiff<K, V>>().mutable.lock_ref().get(&self.key).unwrap())
	}

	pub fn update(&self, f: impl FnOnce(&mut V)) {
		let mut current = self.current();
		f(&mut current);
		self.element.get_cmp::<ChildrenDiff<K, V>>().mutable.lock_mut().insert_cloned(self.key.clone(), current);
	}

	pub fn signal(&self) -> impl hobo::signal::Signal<Item = V> + 'static {
		self.broadcaster.signal_cloned().map({ let initial = self.initial_value.clone(); move |x| x.unwrap_or_else(|| initial.clone()) })
	}
}

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

impl<E, Insert> ChildrenDiffConfig<(), (), E, Insert, fn(), fn(&()), fn(&(), &())> {
	pub fn builder<K, V>() -> ChildrenDiffConfigBuilder<K, V, E, Insert, fn(), fn(&K), fn(&K, &V)> { ChildrenDiffConfigBuilder {
		insert: None,
		on_change: move || {},
		on_remove: move |_| {},
		on_update: move |_, _| {},
		_pd: std::marker::PhantomData,
	} }
}

impl<K, V, E, Insert, OnChange, OnRemove, OnUpdate> ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, OnRemove, OnUpdate> where
	E: hobo::AsElement + 'static,
	Insert: FnMut(&K, SeriousValue<K, V>) -> E + 'static,
	OnChange: FnMut() + 'static,
	OnRemove: FnMut(&K) + 'static,
	OnUpdate: FnMut(&K, &V) + 'static,
{
	#[must_use] pub fn insert(mut self, f: Insert) -> Self { self.insert = Some(f); self }
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
		NewOnUpdate: FnMut(&K, &V) + 'static,
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
	pub mutable: hobo::signal_map::MutableBTreeMap<K, V>,
	/// Element which gets items appended/removed.
	pub element: hobo::Element,
	/// Hobo elements that represent the current state.
	pub items: std::collections::BTreeMap<K, hobo::Element>,
	/// "kind of a hack to avoid running on_change too often"
	unprocessed_ids: std::collections::HashSet<K>,
}

impl<K, V> ChildrenDiff<K, V> where
	K: Ord + Clone + std::hash::Hash + std::fmt::Debug + 'static,
	V: Clone + 'static,
{
	pub fn upsert(&mut self, key: K, value: V) {
		let mut mutable_lock = self.mutable.lock_mut();
		mutable_lock.insert_cloned(key.clone(), value);
		self.unprocessed_ids.insert(key);
	}

	pub fn update_with(&mut self, key: K, f: impl FnOnce(&mut V)) {
		let mut mutable_lock = self.mutable.lock_mut();
		let Some(mut value) = mutable_lock.get(&key).cloned() else { log::warn!("Tried to update non-existing key: {key:?}"); return; };
		f(&mut value);
		mutable_lock.insert_cloned(key.clone(), value);
		self.unprocessed_ids.insert(key);
	}

	pub fn remove(&mut self, key: K) {
		self.mutable.lock_mut().remove(&key);
		self.unprocessed_ids.insert(key);
	}

	pub fn clear(&self) {
		self.mutable.lock_mut().clear();
	}
}

pub trait ChildrenDiffElementExt: AsElement {
	#[must_use]
	fn children_diff<K, V, E, Insert, OnChange, OnRemove, OnUpdate>(self, config: ChildrenDiffConfigBuilder<K, V, E, Insert, OnChange, OnRemove, OnUpdate>) -> Self where
		Self: Sized + Copy + 'static,
		K: Ord + Clone + std::hash::Hash + Send + 'static,
		V: Clone + Send + 'static,
		E: hobo::AsElement + 'static,
		Insert: FnMut(&K, SeriousValue<K, V>) -> E + 'static,
		OnChange: FnMut() + 'static,
		OnRemove: FnMut(&K) + 'static,
		OnUpdate: FnMut(&K, &V) + 'static,
	{
		let ChildrenDiffConfig { mut insert, mut on_change, mut on_remove, mut on_update, .. } = config.build();
		let mutable = MutableBTreeMap::<K, V>::new();
		self
			.component(mutable.signal_map_cloned().subscribe(move |diff| match diff {
				MapDiff::Insert { key, value } => {
					{
						let element = insert(&key, SeriousValue::new(self.as_element(), key.clone(), value)).as_element();
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
				MapDiff::Clear { } => {
					{
						let items = std::mem::take(&mut self.get_cmp_mut::<ChildrenDiff<K, V>>().items);
						for (key, element) in items {
							element.remove();
							on_remove(&key);
						}

						let mut children_diff = self.get_cmp_mut::<ChildrenDiff<K, V>>();
						children_diff.unprocessed_ids.clear();
					}

					on_change();
				},
				MapDiff::Replace { entries } => {
					{
						let items = std::mem::take(&mut self.get_cmp_mut::<ChildrenDiff<K, V>>().items);
						for (key, element) in items {
							element.remove();
							on_remove(&key);
						}

						self.get_cmp_mut::<ChildrenDiff<K, V>>().unprocessed_ids.clear();

						let mut items = std::collections::BTreeMap::<K, hobo::Element>::new();
						for (key, value) in entries {
							let element = insert(&key, SeriousValue::new(self.as_element(), key.clone(), value)).as_element();
							self.add_child(element);
							items.insert(key.clone(), element);
						}

						self.get_cmp_mut::<ChildrenDiff<K, V>>().items = items;
					}

					on_change();
				},
			}))
			.component(ChildrenDiff { mutable, element: self.as_element(), items: Default::default(), unprocessed_ids: Default::default() })
	}
}

impl<T: AsElement> ChildrenDiffElementExt for T {}
