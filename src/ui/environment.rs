//! Dynamic Key/Value editor matching Gearlever's environment-variable rows.

use super::*;

#[derive(Clone)]
pub struct EnvironmentEditor {
    pub widget: gtk::Box,
    rows: Rc<RefCell<Vec<(gtk::Box, gtk::Entry, gtk::Entry)>>>,
    save: gtk::Button,
}

impl EnvironmentEditor {
    pub fn new(save: &gtk::Button) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("card");
        widget.set_margin_top(10);
        Self {
            widget,
            rows: Rc::new(RefCell::new(Vec::new())),
            save: save.clone(),
        }
    }

    pub fn set_values(&self, values: &[(String, String)]) {
        while let Some(child) = self.widget.first_child() {
            self.widget.remove(&child);
        }
        self.rows.borrow_mut().clear();
        for (key, value) in values {
            self.add(key, value);
        }
        self.save.set_sensitive(false);
    }

    pub fn add(&self, key: &str, value: &str) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(10);
        row.set_margin_bottom(10);
        let key_entry = gtk::Entry::builder()
            .placeholder_text(i18n::tr("Key"))
            .text(key)
            .hexpand(true)
            .build();
        let value_entry = gtk::Entry::builder()
            .placeholder_text(i18n::tr("Value"))
            .text(value)
            .hexpand(true)
            .sensitive(!key.is_empty())
            .build();
        let delete = presentation::icon_button("trash", "Remove environment variable");
        delete.add_css_class("destructive-action");
        row.append(&key_entry);
        row.append(&gtk::Label::new(Some("=")));
        row.append(&value_entry);
        row.append(&delete);
        self.widget.append(&row);
        self.rows
            .borrow_mut()
            .push((row.clone(), key_entry.clone(), value_entry.clone()));
        let value_weak = value_entry.downgrade();
        let save_weak = self.save.downgrade();
        key_entry.connect_changed(move |entry| {
            if let Some(value) = value_weak.upgrade() {
                value.set_sensitive(!entry.text().is_empty());
            }
            if let Some(save) = save_weak.upgrade() {
                save.set_sensitive(true);
            }
        });
        let save_weak = self.save.downgrade();
        value_entry.connect_changed(move |_| {
            if let Some(save) = save_weak.upgrade() {
                save.set_sensitive(true);
            }
        });
        let rows = self.rows.clone();
        let parent_weak = self.widget.downgrade();
        let row_weak = row.downgrade();
        let save_weak = self.save.downgrade();
        delete.connect_clicked(move |_| {
            if let (Some(parent), Some(row)) = (parent_weak.upgrade(), row_weak.upgrade()) {
                rows.borrow_mut()
                    .retain(|(existing, _, _)| existing != &row);
                parent.remove(&row);
                if let Some(save) = save_weak.upgrade() {
                    save.set_sensitive(true);
                }
            }
        });
    }

    pub fn values(&self) -> anyhow::Result<Vec<(String, String)>> {
        let mut values = Vec::new();
        for (_, key, value) in self.rows.borrow().iter() {
            let key = key.text().trim().to_string();
            if key.is_empty() {
                continue;
            }
            if key
                .chars()
                .any(|character| character.is_whitespace() || character == '=' || character == '\0')
            {
                anyhow::bail!("{}", i18n::tr("Invalid environment variable"));
            }
            values.push((key, value.text().trim().to_string()));
        }
        Ok(values)
    }
}
