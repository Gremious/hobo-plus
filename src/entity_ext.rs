use hobo::prelude::*;
use futures::future::FutureExt;

pub trait AsEntityExt: AsEntity {
	#[must_use]
	fn bundle<C: 'static>(self, x: C) -> Self where Self: Sized { self.add_bundle(x); self }
	fn add_bundle<C: 'static>(&self, x: C) { self.get_cmp_mut_or_default::<Vec<C>>().push(x) }

	#[expect(clippy::type_complexity)]
	fn get_mutable_write<T: 'static>(&self) -> hobo::owning_ref::OwningHandle<
		hobo::owning_ref::OwningRef<
			StorageGuard<
				hobo::signal::Mutable<T>,
				hobo::owning_ref::OwningRef<
					std::cell::Ref<'static, Box<(dyn DynStorage + 'static)>>,
					Storage<hobo::signal::Mutable<T>>
				>,
			>,
			hobo::signal::Mutable<T>,
		>,
		hobo::signal::MutableLockMut<'static, T>,
	> { hobo::owning_ref::OwningHandle::new_with_fn(self.get_cmp::<hobo::signal::Mutable<T>>(), |x| unsafe { (*x).lock_mut() }) }

	#[expect(clippy::type_complexity)]
	fn get_mutable_read<T: 'static>(&self) -> hobo::owning_ref::OwningHandle<
		hobo::owning_ref::OwningRef<
			StorageGuard<
				hobo::signal::Mutable<T>,
				hobo::owning_ref::OwningRef<
					std::cell::Ref<'static, Box<(dyn DynStorage + 'static)>>,
					Storage<hobo::signal::Mutable<T>>
				>,
			>,
			hobo::signal::Mutable<T>,
		>,
		hobo::signal::MutableLockRef<'static, T>,
	> { hobo::owning_ref::OwningHandle::new_with_fn(self.get_cmp::<hobo::signal::Mutable<T>>(), |x| unsafe { (*x).lock_ref() }) }

	#[track_caller]
	fn spawn_complain<T>(&self, f: impl std::future::Future<Output = anyhow::Result<T>> + 'static) {
		let caller = std::panic::Location::caller();
		let (handle, fut) = hobo::futures_signals::cancelable_future(f.map(|res| if let Err(e) = res {

			let lvl = log::Level::Error;
			if lvl <= log::STATIC_MAX_LEVEL && lvl <= log::max_level() {
				log::__private_api::log(
					log::__private_api::format_args!("{e:?}"),
					lvl,
					&(log::__private_api::module_path!(), log::__private_api::module_path!(), caller),
					(),
				);
			}
		}), Default::default);
		wasm_bindgen_futures::spawn_local(fut);
		self.get_cmp_mut_or_default::<hobo::entity::FutureHandlesCollection>().0.push(handle);
	}

	#[must_use] fn spawn_complain_in<F: FnOnce(&Self) -> Fut, Fut: std::future::Future<Output = anyhow::Result<T>> + 'static, T>(self, f: F) -> Self where Self: Sized { self.spawn_complain(f(&self)); self }
}

impl<T: AsEntity> AsEntityExt for T {}
