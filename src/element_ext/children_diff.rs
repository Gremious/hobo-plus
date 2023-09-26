use hobo::prelude::*;

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

pub trait ChildrenDiffElementExt: AsElement {
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
}

impl<T: AsElement> ChildrenDiffElementExt for T {}
