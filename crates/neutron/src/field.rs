//! A single-line text field: xilem's `text_input`, with the controls the
//! chrome needs and xilem 0.4 does not expose.
//!
//! - **Size and face.** The inner `TextArea` gets an explicit font size and the
//!   app's font stack, instead of Masonry's fixed 15 px system face.
//! - **Focus.** The field reports gaining and losing focus, and Escape gives
//!   focus up (the text area passes Escape through unhandled).
//! - **Its own frame.** Masonry's `TextInput` paints its focused border in
//!   hard-coded white, which disappears on a light palette. Here the inner
//!   input is transparent and borderless, and the wrapper paints the ground,
//!   the border and a focus ring from the theme.
//!
//! The widget is a thin wrapper, [`FieldWidget`], around Masonry's
//! `TextInput`; the view mirrors xilem's `TextInput` view and adds the rest.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, ArcStr, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    NewWidget, PaintCtx, PointerEvent, Properties, PropertiesMut,
    PropertiesRef, RegisterCtx, StyleProperty, TextEvent, Update, UpdateCtx,
    Widget, WidgetMut, WidgetPod,
    keyboard::{Key, KeyState, NamedKey},
  },
  kurbo::{Affine, RoundedRect, Size, Stroke},
  peniko::{Brush, Color, Fill},
  properties::{
    Background, BorderWidth, CaretColor, ContentColor, PlaceholderColor,
  },
  vello::Scene,
  widgets::{self, TextAction},
};
use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

use crate::{
  focus::{self, FieldKey},
  font,
  theme::Theme,
  tokens::radius,
};

/// What the wrapper itself reports, alongside the text area's own
/// [`TextAction`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldAction {
  /// The field gained or lost keyboard focus.
  Focus {
    /// Whether the field now has focus.
    focused:    bool,
    /// Whether a click in the field caused it (rather than the key map).
    by_pointer: bool,
  },
}

// --- the widget ---------------------------------------------------------

/// Paints the field's frame around a transparent Masonry `TextInput`, and
/// reports focus changes.
pub struct FieldWidget {
  child:   WidgetPod<widgets::TextInput>,
  colors:  Colors,
  /// Set between a press inside the field and the focus change it causes,
  /// so the view can tell a click from keyboard focus.
  pressed: bool,
}

/// The frame's colours, all taken from the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Colors {
  ground: Color,
  border: Color,
  focus:  Color,
}

impl Colors {
  fn from_theme(theme: &Theme) -> Self {
    Self {
      ground: theme.sunken,
      border: theme.rule,
      focus:  theme.focus,
    }
  }
}

impl FieldWidget {
  fn new(child: NewWidget<widgets::TextInput>, colors: Colors) -> Self {
    Self {
      child: child.to_pod(),
      colors,
      pressed: false,
    }
  }

  fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, widgets::TextInput> {
    this.ctx.get_mut(&mut this.widget.child)
  }
}

impl Widget for FieldWidget {
  type Action = FieldAction;

  fn on_text_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &TextEvent,
  ) {
    // Escape bubbles up from the text area unhandled: treat it as "done
    // here", so the key map sees the next Escape.
    if let TextEvent::Keyboard(key) = event
      && key.state == KeyState::Down
      && key.key == Key::Named(NamedKey::Escape)
      && ctx.has_focus_target()
    {
      ctx.resign_focus();
      ctx.set_handled();
    }
  }

  fn on_pointer_event(
    &mut self,
    _ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    // Presses bubble up from the text area before focus moves to it.
    match event {
      PointerEvent::Down(_) => self.pressed = true,
      PointerEvent::Up(_) | PointerEvent::Cancel(_) => self.pressed = false,
      _ => {}
    }
  }

  fn update(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &Update,
  ) {
    if let Update::ChildFocusChanged(focused) = event {
      ctx.submit_action::<FieldAction>(FieldAction::Focus {
        focused:    *focused,
        by_pointer: std::mem::take(&mut self.pressed),
      });
      ctx.request_paint_only();
    }
  }

  fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
    ctx.register_child(&mut self.child);
  }

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> Size {
    let size = ctx.run_layout(&mut self.child, bc);
    ctx.place_child(&mut self.child, (0.0, 0.0).into());
    size
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let rect = ctx.size().to_rect();
    let shape = RoundedRect::from_rect(rect.inset(-0.5), radius::CONTROL);
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(self.colors.ground),
      None,
      &shape,
    );
    let (color, width) = if ctx.has_focus_target() {
      (self.colors.focus, 2.0)
    } else {
      (self.colors.border, 1.0)
    };
    // Stroke inside the bounds, so the ring is never clipped by a parent.
    let ring =
      RoundedRect::from_rect(rect.inset(-width / 2.0), radius::CONTROL);
    scene.stroke(
      &Stroke::new(width),
      Affine::IDENTITY,
      &Brush::Solid(color),
      None,
      &ring,
    );
  }

  fn accessibility_role(&self) -> Role { Role::GenericContainer }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    _node: &mut AccessNode,
  ) {
  }

  fn children_ids(&self) -> ChildrenIds {
    ChildrenIds::from_slice(&[self.child.id()])
  }
}

// --- the view -----------------------------------------------------------

type Callback<State, Action, T> =
  Box<dyn Fn(&mut State, T) -> Action + Send + Sync + 'static>;

/// A text field showing `contents`, reporting every edit to `on_changed`.
/// As with xilem's `text_input`, the contents must live in app state.
pub fn field<State, Action, F>(
  contents: String,
  theme: &'static Theme,
  on_changed: F,
) -> Field<State, Action>
where
  F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
{
  Field {
    contents,
    theme,
    size: crate::tokens::text::BODY,
    placeholder: ArcStr::default(),
    on_changed: Box::new(on_changed),
    on_enter: None,
    focus_key: None,
  }
}

/// The view created by [`field`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Field<State, Action> {
  contents:    String,
  theme:       &'static Theme,
  size:        f32,
  placeholder: ArcStr,
  on_changed:  Callback<State, Action, String>,
  on_enter:    Option<Callback<State, Action, String>>,
  focus_key:   Option<FieldKey>,
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

  /// Let the key map focus this field by `key` (see [`crate::focus`]).
  pub fn focus_key(mut self, key: FieldKey) -> Self {
    self.focus_key = Some(key);
    self
  }

  /// The inner input's properties: transparent and borderless, since the
  /// wrapper paints the frame.
  fn input_props(&self) -> Properties {
    let mut props = Properties::new();
    props.insert(Background::Color(Color::TRANSPARENT));
    props.insert(BorderWidth { width: 0.0 });
    props.insert(PlaceholderColor::new(self.theme.muted));
    props.insert(CaretColor {
      color: self.theme.text,
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
      color: self.theme.text,
    });
    let area = widgets::TextArea::new_editable(&self.contents)
      .with_style(StyleProperty::FontSize(self.size))
      .with_style(StyleProperty::FontStack(font::STACK));
    let input = widgets::TextInput::from_text_area(NewWidget::new_with_props(
      area, area_props,
    ))
    .with_placeholder(self.placeholder.clone());
    // The text area's edits are routed to this view, as are the wrapper's
    // focus reports.
    let area_id = input.area_pod().id();
    ctx.record_action(area_id);
    if let Some(key) = self.focus_key {
      focus::register(key, area_id);
    }
    let input = NewWidget::new_with_props(input, self.input_props());
    let pod = ctx.with_action_widget(|ctx| {
      ctx.create_pod(FieldWidget::new(input, Colors::from_theme(self.theme)))
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
    if !std::ptr::eq(self.theme, prev.theme) {
      element.widget.colors = Colors::from_theme(self.theme);
      element.ctx.request_paint_only();
    }
    let mut input = FieldWidget::child_mut(&mut element);
    if !std::ptr::eq(self.theme, prev.theme) {
      input.insert_prop(PlaceholderColor::new(self.theme.muted));
      input.insert_prop(CaretColor {
        color: self.theme.text,
      });
    }
    if self.placeholder != prev.placeholder {
      widgets::TextInput::set_placeholder(&mut input, self.placeholder.clone());
    }
    let mut area = widgets::TextInput::text_mut(&mut input);
    if !std::ptr::eq(self.theme, prev.theme) {
      area.insert_prop(ContentColor {
        color: self.theme.text,
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
      if let Some(key) = self.focus_key {
        focus::unregister(key, area.ctx.widget_id());
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
    match message.take_message::<FieldAction>().map(|a| *a) {
      // Focus from the keyboard selects the whole text, so typing replaces
      // it, as a rename field should; a click keeps the caret where it
      // landed.
      Some(FieldAction::Focus {
        focused: true,
        by_pointer: false,
      }) => {
        let mut input = FieldWidget::child_mut(&mut element);
        let mut area = widgets::TextInput::text_mut(&mut input);
        let len: usize = area.widget.text().into_iter().map(str::len).sum();
        widgets::TextArea::select_byte_range(&mut area, 0, len);
        MessageResult::Nop
      }
      Some(_) => MessageResult::Nop,
      None => MessageResult::Stale,
    }
  }
}
