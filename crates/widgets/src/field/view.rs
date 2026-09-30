//! The Xilem view that hosts [`FieldWidget`], mirroring xilem's own
//! `TextInput` view.

use masonry::{
  core::{ArcStr, NewWidget, Properties, StyleProperty},
  peniko::Color,
  properties::{
    Background, BorderWidth, CaretColor, ContentColor, PlaceholderColor,
  },
  widgets::{self, TextAction},
};
use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

use super::{FieldAction, Look, Mount, widget::FieldWidget};

type Callback<State, Action, T> =
  Box<dyn Fn(&mut State, T) -> Action + Send + Sync + 'static>;

/// A text field showing `contents`, reporting every edit to `on_changed`.
/// As with xilem's `text_input`, the contents must live in app state.
pub fn field<State, Action, F>(
  contents: String,
  look: Look,
  size: f32,
  on_changed: F,
) -> Field<State, Action>
where
  F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
{
  Field {
    contents,
    look,
    size,
    placeholder: ArcStr::default(),
    on_changed: Box::new(on_changed),
    on_enter: None,
    mount: None,
    on_focus: None,
    escape_bubbles: false,
  }
}

/// The view created by [`field`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Field<State, Action> {
  contents:       String,
  look:           Look,
  size:           f32,
  placeholder:    ArcStr,
  on_changed:     Callback<State, Action, String>,
  on_enter:       Option<Callback<State, Action, String>>,
  mount:          Option<Box<dyn Mount>>,
  on_focus:       Option<Callback<State, Action, bool>>,
  escape_bubbles: bool,
}

impl<State, Action> Field<State, Action> {
  /// Set the text size, in logical pixels.
  pub fn size(mut self, size: f32) -> Self {
    self.size = size;
    self
  }

  /// Text shown, muted, while the field is empty.
  pub fn placeholder(mut self, text: impl Into<ArcStr>) -> Self {
    self.placeholder = text.into();
    self
  }

  /// Called with the contents when Enter is pressed.
  pub fn on_enter<F>(mut self, f: F) -> Self
  where
    F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
  {
    self.on_enter = Some(Box::new(f));
    self
  }

  /// Called with `true` when the field gains focus and `false` when it
  /// loses it.
  pub fn on_focus<F>(mut self, f: F) -> Self
  where
    F: Fn(&mut State, bool) -> Action + Send + Sync + 'static,
  {
    self.on_focus = Some(Box::new(f));
    self
  }

  /// Let Escape go on to the key map after leaving the field, rather than
  /// stopping there: for a field whose Escape should also end a mode.
  pub fn escape_bubbles(mut self, bubbles: bool) -> Self {
    self.escape_bubbles = bubbles;
    self
  }

  /// Report the text area's id as it is mounted and torn down, so the app
  /// can find this field again when something asks for focus.
  pub fn mount(mut self, mount: impl Mount) -> Self {
    self.mount = Some(Box::new(mount));
    self
  }

  /// The inner input's properties: transparent and borderless, since the
  /// wrapper paints the frame.
  fn input_props(&self) -> Properties {
    let mut props = Properties::new();
    props.insert(Background::Color(Color::TRANSPARENT));
    props.insert(BorderWidth { width: 0.0 });
    props.insert(PlaceholderColor::new(self.look.muted));
    props.insert(CaretColor {
      color: self.look.text,
    });
    props
  }
}

impl<State, Action> ViewMarker for Field<State, Action> {}
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx>
  for Field<State, Action>
{
  type Element = Pod<FieldWidget>;
  type ViewState = ();

  fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
    let mut area_props = Properties::new();
    area_props.insert(ContentColor {
      color: self.look.text,
    });
    let area = widgets::TextArea::new_editable(&self.contents)
      .with_style(StyleProperty::FontSize(self.size))
      .with_style(StyleProperty::FontStack(self.look.font.clone()));
    let input = widgets::TextInput::from_text_area(NewWidget::new_with_props(
      area, area_props,
    ))
    .with_placeholder(self.placeholder.clone());
    // The text area's edits are routed to this view, as are the wrapper's
    // focus reports.
    let area_id = input.area_pod().id();
    ctx.record_action(area_id);
    if let Some(mount) = &self.mount {
      mount.mounted(area_id);
    }
    let input = NewWidget::new_with_props(input, self.input_props());
    let pod = ctx.with_action_widget(|ctx| {
      ctx.create_pod(FieldWidget::new(
        input,
        self.look.clone(),
        self.escape_bubbles,
      ))
    });
    (pod, ())
  }

  fn rebuild(
    &self,
    prev: &Self,
    _: &mut (),
    _ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    _: &mut State,
  ) {
    FieldWidget::set_escape_bubbles(&mut element, self.escape_bubbles);
    let restyled = self.look != prev.look;
    if restyled {
      FieldWidget::set_colors(&mut element, self.look.clone());
    }
    let mut input = FieldWidget::child_mut(&mut element);
    if restyled {
      input.insert_prop(PlaceholderColor::new(self.look.muted));
      input.insert_prop(CaretColor {
        color: self.look.text,
      });
    }
    if self.placeholder != prev.placeholder {
      widgets::TextInput::set_placeholder(&mut input, self.placeholder.clone());
    }
    let mut area = widgets::TextInput::text_mut(&mut input);
    if restyled {
      area.insert_prop(ContentColor {
        color: self.look.text,
      });
    }
    if self.size != prev.size {
      widgets::TextArea::insert_style(
        &mut area,
        StyleProperty::FontSize(self.size),
      );
    }
    // As in xilem's view: compare against the widget's text, not the
    // previous view, so a keystroke already in the widget is not reset.
    if area.widget.text() != &self.contents {
      widgets::TextArea::reset_text(&mut area, &self.contents);
    }
  }

  fn teardown(
    &self,
    _: &mut (),
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
  ) {
    {
      let mut input = FieldWidget::child_mut(&mut element);
      let area = widgets::TextInput::text_mut(&mut input);
      if let Some(mount) = &self.mount {
        mount.unmounted(area.ctx.widget_id());
      }
      ctx.teardown_leaf(area);
    }
    ctx.teardown_leaf(element);
  }

  fn message(
    &self,
    _: &mut (),
    message: &mut MessageContext,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    if let Some(action) = message.take_message::<TextAction>() {
      return match *action {
        TextAction::Changed(text) => {
          MessageResult::Action((self.on_changed)(app_state, text))
        }
        TextAction::Entered(text) => match &self.on_enter {
          Some(f) => MessageResult::Action(f(app_state, text)),
          None => MessageResult::Nop,
        },
      };
    }
    let Some(FieldAction::Focus {
      focused,
      by_pointer,
    }) = message.take_message::<FieldAction>().map(|a| *a)
    else {
      return MessageResult::Stale;
    };
    // Focus from the keyboard selects the whole text, so typing replaces it,
    // as a rename field should; a click keeps the caret where it landed.
    if focused && !by_pointer {
      FieldWidget::select_all(&mut element);
    }
    match &self.on_focus {
      Some(f) => MessageResult::Action(f(app_state, focused)),
      None => MessageResult::Nop,
    }
  }
}
