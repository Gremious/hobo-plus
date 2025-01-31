use std::{cell::RefCell, rc::Rc};

use wasm_bindgen_futures::js_sys;
use serde::{Serialize, de::DeserializeOwned};
use hobo::prelude::*;

// static URL: Lazy<String> = Lazy::new(|| format!("wss://{}/ws", shared::CONFIG.host.name));
// pub static SOCKET: Lazy<Socket> = Lazy::new(Socket::new);

pub struct Socket<In, Out> {
	ws: Rc<RefCell<web_sys::WebSocket>>,
	onopen: Rc<Closure<dyn Fn(web_sys::Event)>>,
	onclose: Rc<RefCell<Option<Closure<dyn Fn(web_sys::CloseEvent)>>>>,
	onmessage: Rc<Closure<dyn Fn(web_sys::MessageEvent)>>,
	// this should probably be bounded
	message_buffer: RefCell<Vec<Out>>,

	message_handler: fn(In),
}

unsafe impl<In, Out> Send for Socket<In, Out> {}
unsafe impl<In, Out> Sync for Socket<In, Out> {}

// pub fn send(msg: ClientMessage) -> anyhow::Result<()> {
//     let send_res = SOCKET.ws.borrow().send_with_u8_array(&postcard::to_stdvec(&msg).unwrap()).map_err(|e| anyhow::anyhow!("{e:?}"));
//     if send_res.is_err() { SOCKET.message_buffer.borrow_mut().push(msg); }
//     send_res
// }

// #[expect(clippy::needless_pass_by_value)]
fn on_message<In: DeserializeOwned + 'static>(handle_message: impl Fn(In)) -> impl Fn(web_sys::MessageEvent) {
	move |e: web_sys::MessageEvent| {
		let u8_arr = js_sys::Uint8Array::new(&e.data());
		let msg = match postcard::from_bytes::<In>(&u8_arr.to_vec()) {
			Ok(x) => x,
			Err(e) => { log::error!("Error deserializing server message: {e:?}"); return; },
		};
		handle_message(msg);
	}
}

fn on_open(_: web_sys::Event) {
	// TODO:
	/*
	// log::info!("onopen: {:#?}", js_sys::JSON::stringify(&e));
	let hello_res = send(ClientMessage::Hello(Hello::FromWeb));
	log::info!("Socket Initialized: {hello_res:?}");
	let buffer = std::mem::take(&mut SOCKET.message_buffer.borrow_mut() as &mut Vec<_>);
	for msg in buffer { send(msg).ok(); }
	*/
}

fn on_close(_: web_sys::CloseEvent) {
	// log::info!("onclose: {:#?}", e);
	let mut interval = async_timer::interval(std::time::Duration::from_secs(5));
	wasm_bindgen_futures::spawn_local(async move { loop {
		// log::info!("try again");
		interval.wait().await;
		// TODO:
		/*
		match web_sys::WebSocket::new(&URL) {
			Ok(new_ws) => {
				new_ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
				new_ws.set_onmessage(Some(SOCKET.onmessage.as_ref().unchecked_ref()));
				new_ws.set_onopen(Some(SOCKET.onopen.as_ref().unchecked_ref()));
				new_ws.set_onclose(Some(SOCKET.onclose.as_ref().unchecked_ref()));
				let mut ws = SOCKET.ws.borrow_mut();
				*ws = new_ws;
				break;
			},
			// this is very unlikely to happen
			Err(e) => log::warn!("{e:?}"),
		}
		*/
	} });
}

impl<In: DeserializeOwned + 'static, Out: Serialize> Socket<In, Out> {
	pub fn new(url: &str, message_handler: fn(In)) -> Self {
		let ws = Rc::new(RefCell::new(web_sys::WebSocket::new(url).unwrap()));
		let message_buffer = RefCell::new(Vec::new());

		let onopen = Rc::new(Closure::new(on_open));
		let onmessage = Rc::new(Closure::new(on_message(message_handler)));
		let onclose = Rc::new(RefCell::new(None as Option<Closure<dyn Fn(web_sys::CloseEvent)>>));
		*onclose.borrow_mut() = Some(Closure::new(#[clown::clown] |e: web_sys::CloseEvent| {
			let url = honk!(url.to_owned()).clone();
			let ws = slip!(Rc::downgrade(&ws)).clone();

			let mut interval = async_timer::interval(std::time::Duration::from_secs(5));
			wasm_bindgen_futures::spawn_local(async move { loop {
				// log::info!("try again");
				interval.wait().await;
				let Some(ws) = ws.upgrade() else { break; };
				match web_sys::WebSocket::new(&url) {
					Ok(new_ws) => {
						let mut ws = ws.borrow_mut();
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
		}));

		{
			let ws = ws.borrow_mut();
			ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
			ws.set_onopen(Some((*onopen).as_ref().unchecked_ref()));
			ws.set_onmessage(Some((*onmessage).as_ref().unchecked_ref()));
			ws.set_onclose(Some(onclose.borrow().as_ref().unwrap().as_ref().unchecked_ref()));
		}

		// let ws = RefCell::new(ws);
		Self { ws, onopen, onclose, onmessage, message_buffer, message_handler }
	}

	#[culpa::throws(anyhow::Error)]
	pub fn send(&self, msg: Out) {
		let send_res = self.ws.borrow().send_with_u8_array(&postcard::to_stdvec(&msg)?).map_err(|e| anyhow::anyhow!("{e:?}"));
		if send_res.is_err() { self.message_buffer.borrow_mut().push(msg); }
		send_res?;
	}
}
