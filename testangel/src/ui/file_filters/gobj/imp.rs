use std::cell::RefCell;

use relm4::gtk::{FileFilter, gio, glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Default)]
pub struct FileFilterListModel {
    pub(super) inner: RefCell<Vec<FileFilter>>,
}

/// Basic declaration of our type for the `GObject` type system
#[glib::object_subclass]
impl ObjectSubclass for FileFilterListModel {
    const NAME: &'static str = "TestAngelFileFilterListModel";
    type Type = super::FileFilterListModel;
    type ParentType = glib::Object;
    type Interfaces = (gio::ListModel,);
}

impl ObjectImpl for FileFilterListModel {}

impl ListModelImpl for FileFilterListModel {
    fn item_type(&self) -> glib::Type {
        FileFilter::static_type()
    }

    fn n_items(&self) -> u32 {
        u32::try_from(self.inner.borrow().len()).unwrap()
    }

    fn item(&self, position: u32) -> Option<glib::Object> {
        self.inner
            .borrow()
            .get(position as usize)
            .map(|o| o.clone().upcast::<glib::Object>())
    }
}
