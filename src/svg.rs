pub use crate::document;

pub fn xml_to_svg(xml_node: &roxmltree::Node) -> web_sys::SvgElement {
	let html_node: web_sys::SvgElement = wasm_bindgen::JsCast::unchecked_into(document().create_element_ns(Some(wasm_bindgen::intern("http://www.w3.org/2000/svg")), xml_node.tag_name().name()).unwrap());
	for attribute in xml_node.attributes() {
		html_node.set_attribute(wasm_bindgen::intern(attribute.name()), attribute.value()).unwrap();
	}
	for child in xml_node.children().filter(roxmltree::Node::is_element) {
		html_node.append_child(&xml_to_svg(&child)).unwrap();
	}
	html_node
}

#[macro_export]
macro_rules! __svgs {
	($base:expr, $($name:ident => $address:expr),*$(,)*) => {$(
		#[must_use]
		pub fn $name() -> hobo::create::Svg {
			thread_local! { static TEMPLATE: web_sys::SvgElement = $crate::svg::xml_to_svg(&roxmltree::Document::parse(include_str!(concat!($base, $address))).unwrap().root_element()) }
			let element: web_sys::SvgElement = wasm_bindgen::JsCast::dyn_into(TEMPLATE.with(|x| x.clone_node_with_deep(true).unwrap())).unwrap();
			hobo::create::Svg(hobo::create::svg_element(&element))
		}
	)*};
}
