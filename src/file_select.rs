use hobo::{prelude::*, create as e};
use super::document;

struct FileSelect {
	element: e::Input,
	file_load_future: Option<std::pin::Pin<Box<wasm_bindgen_futures::JsFuture>>>,
}

#[derive(Default, Clone, PartialEq, Eq)]
enum TaskState {
	#[default]
	FirstPoll,
	WaitingForFileSelect,
	Errored(FileError),
	LoadFile,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum FileError {
	#[error("File size exceeded 2 MB.")] FileTooBig,
	#[error("File selection canceled.")] Canceled,
	#[error("Failed to load file: '{0}'.")] JsFileLoadError(String),
}

pub struct UserFile {
	pub js_object: web_sys::File,
	pub bytes: Vec<u8>,
}

impl std::future::Future for FileSelect {
	type Output = Result<UserFile, FileError>;

	fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
		let input = self.element;
		let task_state = input.get_cmp::<TaskState>().clone();

		match task_state {
			TaskState::FirstPoll => {
				input
					.on_change(#[clown::clown] |_| {
						let Some(file) = input.get_cmp::<web_sys::HtmlInputElement>().files().unwrap().item(0) else { return; };

						if file.size() > 2_000_000. {
							*input.get_cmp_mut::<TaskState>() = TaskState::Errored(FileError::FileTooBig);
						} else {
							input.add_component(Some(file));
							*input.get_cmp_mut::<TaskState>() = TaskState::LoadFile;
						}

						honk!(cx.waker()).wake_by_ref();
					})
					.add_component(document().on_focus(#[clown::clown] |_| {
						let waker = honk!(cx.waker()).clone();
						input.spawn(async move {
							async_timer::interval(std::time::Duration::from_secs(1)).wait().await;

							// Check this state instead of checking for file to avoid using the dom
							let mut task_state = input.get_cmp_mut::<TaskState>();
							if *task_state == TaskState::WaitingForFileSelect {
								*task_state = TaskState::Errored(FileError::Canceled);
								waker.wake_by_ref();
							};
						});
					}));

				*input.get_cmp_mut::<TaskState>() = TaskState::WaitingForFileSelect;
				input.get_cmp::<web_sys::HtmlInputElement>().click();
			},
			TaskState::WaitingForFileSelect => return std::task::Poll::Pending,
			TaskState::Errored(e) => return std::task::Poll::Ready(Err(e)),
			TaskState::LoadFile => {
				let mut file = input.get_cmp_mut::<Option<web_sys::File>>();

				let future = if let Some(x) = self.file_load_future.as_mut() { x } else {
					let promise = (*file).as_mut().unwrap().array_buffer();
					let future = Box::pin(wasm_bindgen_futures::JsFuture::from(promise));
					self.file_load_future = Some(future);
					self.file_load_future.as_mut().unwrap()
				};

				return future.as_mut().poll(cx)
					.map_ok(|js_val| UserFile {
						js_object: file.take().unwrap(),
						bytes: js_sys::Uint8Array::new(&js_val).to_vec(),
					})
					.map_err(|js_err| FileError::JsFileLoadError(js_err.as_string().unwrap()));
			},
		}

		std::task::Poll::Pending
	}
}

impl Drop for FileSelect {
	fn drop(&mut self) { self.element.remove() }
}

pub async fn open(accept: &str) -> Result<UserFile, FileError> {
	FileSelect {
		element: e::input()
			.type_file()
			.attr(web_str::accept(), accept)
			.component(TaskState::default())
			.allow_no_parent(),
		file_load_future: None,
	}.await
}
