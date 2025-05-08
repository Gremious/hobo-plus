use std::{cell::RefCell, rc::Rc};
use wasm_bindgen_futures::js_sys;
use serde::{Serialize, de::DeserializeOwned};
use hobo::prelude::*;
#[allow(unused_imports)] use super::{honk, slip};

pub struct Socket<Out> {
	ws: Rc<RefCell<web_sys::WebSocket>>,
	// this should probably be bounded
	message_buffer: Rc<RefCell<Vec<Out>>>,
}

unsafe impl<Out> Send for Socket<Out> {}
unsafe impl<Out> Sync for Socket<Out> {}

impl<Out: Serialize + 'static> Socket<Out> {
	pub fn new<In: DeserializeOwned + 'static>(url: &str, on_open: fn(&Self), on_message: fn(&Self, In)) -> Self {
		let ws = Rc::new(RefCell::new(web_sys::WebSocket::new(url).unwrap()));
		let message_buffer = Rc::new(RefCell::new(Vec::new()));

		let onopen = Closure::<dyn Fn(web_sys::Event)>::new(#[clown::clown] |_: web_sys::Event| {
			let Some(ws) = slip!(Rc::downgrade(&ws)).upgrade() else { return; };
			let Some(message_buffer) = slip!(Rc::downgrade(&message_buffer)).upgrade() else { return; };

			let this = Self { ws: Rc::clone(&ws), message_buffer: Rc::clone(&message_buffer) };
			on_open(&this);

			let buffer = std::mem::take(&mut message_buffer.borrow_mut() as &mut Vec<_>);
			for msg in buffer { this.send(msg).ok(); }
		}).into_js_value();
		let onmessage = Closure::<dyn Fn(web_sys::MessageEvent)>::new(#[clown::clown] |e: web_sys::MessageEvent| {
			let Some(ws) = slip!(Rc::downgrade(&ws)).upgrade() else { return; };
			let Some(message_buffer) = slip!(Rc::downgrade(&message_buffer)).upgrade() else { return; };

			let u8_arr = js_sys::Uint8Array::new(&e.data());
			let msg = match postcard::from_bytes::<In>(&u8_arr.to_vec()) {
				Ok(x) => x,
				Err(e) => { log::error!("Error deserializing server message: {e:?}"); return; },
			};

			let this = Self { ws: Rc::clone(&ws), message_buffer: Rc::clone(&message_buffer) };
			on_message(&this, msg);
		}).into_js_value();
		let onclose = Closure::<dyn Fn(web_sys::CloseEvent)>::new(#[clown::clown] |_: web_sys::CloseEvent| {
			let ws = slip!(Rc::downgrade(&ws)).clone();

			let mut interval = async_timer::interval(std::time::Duration::from_secs(5));
			wasm_bindgen_futures::spawn_local(async move { loop {
				// log::info!("socket closed, try again");
				interval.wait().await;
				let Some(ws) = ws.upgrade() else { break; };
				let mut ws = ws.borrow_mut();
				match web_sys::WebSocket::new(&ws.url()) {
					Ok(new_ws) => {
						new_ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
						new_ws.set_onopen(ws.onopen().as_ref());
						new_ws.set_onmessage(ws.onmessage().as_ref());
						new_ws.set_onclose(ws.onclose().as_ref());
						*ws = new_ws;
						break;
					},
					// this is very unlikely to happen
					Err(e) => log::warn!("{e:?}"),
				}
			} });
		}).into_js_value();

		{
			let ws = ws.borrow_mut();
			ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
			ws.set_onopen(Some(onopen.unchecked_ref()));
			ws.set_onmessage(Some(onmessage.unchecked_ref()));
			ws.set_onclose(Some(onclose.unchecked_ref()));
		}

		Self { ws, message_buffer }
	}

	#[culpa::throws(anyhow::Error)]
	pub fn send(&self, msg: Out) {
		let ws = self.ws.borrow();
		if ws.ready_state() != web_sys::WebSocket::OPEN {
			log::warn!("failed to send, buffering: status is not web_sys::WebSocket::OPEN");
			self.message_buffer.borrow_mut().push(msg);
			return;
		}
		let send_res = ws.send_with_u8_array(&postcard::to_stdvec(&msg)?).map_err(|e| anyhow::anyhow!("{e:?}"));
		if send_res.is_err() {
			log::warn!("failed to send, buffering");
			self.message_buffer.borrow_mut().push(msg);
		}
		send_res?;
	}
}
