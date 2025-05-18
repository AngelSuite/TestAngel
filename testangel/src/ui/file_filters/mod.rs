use relm4::gtk::{self, FileFilter};

use super::lang;

mod gobj;
use gobj::FileFilterListModel;

/// Get a [`FileFilter`] tuned to all files.
pub fn all() -> FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&lang::lookup("filetype-all")));
    filter.add_pattern("*");
    filter
}

/// Get a [`FileFilter`] tuned to flows.
pub fn flows() -> FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&lang::lookup("filetype-flow")));
    filter.add_suffix("taflow");
    filter
}

/// Get a [`FileFilter`] tuned to actions.
pub fn actions() -> FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&lang::lookup("filetype-action")));
    filter.add_suffix("taaction");
    filter
}

/// Get a [`FileFilter`] tuned to EVPs.
pub fn evps() -> FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&lang::lookup("filetype-evp")));
    filter.add_suffix("evp");
    filter
}

/// Get a [`FileFilter`] tuned to CSVs.
pub fn csvs() -> FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&lang::lookup("filetype-csv")));
    filter.add_suffix("csv");
    filter
}

/// Create a [`FileFilterListModel`] containing the provided list of [`FileFilter`]s.
pub fn filter_list(filters: &[FileFilter]) -> FileFilterListModel {
    let model = FileFilterListModel::new();
    filters.iter().for_each(|f| model.append(f.clone()));
    model
}
