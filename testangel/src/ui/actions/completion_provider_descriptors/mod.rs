use glib::Object;
use gtk::glib;
use relm4::gtk;
use sourceview5::CompletionProvider;

mod imp;
mod item;

glib::wrapper! {
    pub struct CompletionProviderDescriptors(ObjectSubclass<imp::CompletionProviderDescriptors>)
        @implements CompletionProvider;
}

impl CompletionProviderDescriptors {
    /// Create a new [`CompletionProvider`] that suggests engines.
    pub fn new() -> Self {
        let obj: Self = Object::builder().build();
        obj
    }
}

impl Default for CompletionProviderDescriptors {
    fn default() -> Self {
        Self::new()
    }
}
