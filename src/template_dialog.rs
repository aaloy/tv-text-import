//! The dialog that picks a template for the file about to be imported.

use turbo_vision::app::Application;
use turbo_vision::core::command::CM_OK;
use turbo_vision::views::GroupLike;
use turbo_vision::views::combo_box::ComboBox;
use tv_extensions::forms::{Form, size};

use crate::template::Template;

/// The `ComboBox` id the dialog uses. Combo box ids must be unique among the
/// combo boxes alive at the same time; this does not overlap the other
/// dialogs'.
pub const TEMPLATE_COMBO_ID: u16 = 0xE300;

/// The entry for importing without a template.
const NONE: &str = "(none - set the options by hand)";

/// Asks which of `templates` to import with.
///
/// Returns `None` on Cancel or Esc, `Some(None)` for "none", and
/// `Some(Some(template))` for a template.
pub fn choose_template<'a>(
    app: &mut Application,
    title: &str,
    templates: &'a [Template],
) -> Option<Option<&'a Template>> {
    let mut items = vec![NONE.to_string()];
    items.extend(templates.iter().map(|t| t.name.clone()));
    let width = i16::try_from(items.iter().map(|s| s.chars().count()).max().unwrap_or(10))
        .unwrap_or(i16::MAX);

    let mut form = Form::new(title);
    let combo = form.field(
        "~T~emplate",
        ComboBox::with_items(size(width, 1), TEMPLATE_COMBO_ID, items),
    );
    form.ok_cancel();
    let mut dialog = form.build();
    dialog.prepare_modal(app);
    if let Some(c) = dialog.get_mut(combo) {
        c.set_selected(Some(usize::from(!templates.is_empty())));
    }
    if app.execute_modal(&mut dialog, |_, _| turbo_vision::app::ModalTick::Continue) != CM_OK {
        return None;
    }
    let picked = dialog.get(combo).and_then(ComboBox::selected).unwrap_or(0);
    Some(picked.checked_sub(1).and_then(|i| templates.get(i)))
}
