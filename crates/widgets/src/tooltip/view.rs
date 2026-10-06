//! The Xilem view that hosts [`TooltipWidget`].

use masonry::core::{NewWidget, Widget, WidgetMut};

use super::{Anchor, Look, TooltipWidget};
use crate::wrap::{Wrap, Wrapper, wrap};

/// `child` with a tooltip reading `text`.
pub fn tooltip<V>(
  text: impl Into<String>,
  look: Look,
  anchor: Anchor,
  child: V,
) -> Tooltip<V> {
  wrap(
    TooltipProps {
      text: text.into(),
      look,
      anchor,
    },
    child,
  )
}

/// The view created by [`tooltip`].
pub type Tooltip<V> = Wrap<TooltipProps, V>;

/// What a [`Tooltip`] is built from.
pub struct TooltipProps {
  text: String,
  look: Look,
  anchor: Anchor,
}

impl<State, Action> Wrapper<State, Action> for TooltipProps {
  type Widget = TooltipWidget;

  fn build(&self, child: NewWidget<dyn Widget>) -> TooltipWidget {
    TooltipWidget::new(child, self.text.clone(), self.anchor, self.look.clone())
  }

  fn rebuild(&self, prev: &Self, widget: &mut WidgetMut<'_, TooltipWidget>) {
    let changed = self.text != prev.text
      || self.anchor != prev.anchor
      || self.look != prev.look;
    if changed {
      TooltipWidget::relabel(
        widget,
        self.text.clone(),
        self.anchor,
        self.look.clone(),
      );
    }
  }

  fn child_mut<'t>(
    widget: &'t mut WidgetMut<'_, TooltipWidget>,
  ) -> WidgetMut<'t, dyn Widget> {
    TooltipWidget::child_mut(widget)
  }

  // The layer lives outside this widget's tree, so it would outlive it.
  fn teardown(widget: &mut WidgetMut<'_, TooltipWidget>) {
    TooltipWidget::hide(widget);
  }
}
