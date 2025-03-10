use crate::{
    command::{
        CommandTarget, LapceCommandNew, LapceUICommand, LapceWorkbenchCommand,
        LAPCE_NEW_COMMAND, LAPCE_UI_COMMAND,
    },
    config::{Config, LapceTheme},
    data::{
        EditorContent, FocusArea, LapceEditorData, LapceTabData, PanelData,
        PanelKind,
    },
    editor::{EditorLocation, LapceEditorView},
    keypress::{DefaultKeyPressHandler, KeyPress},
    scroll::{LapcePadding, LapceScroll},
    svg::logo_svg,
    terminal::{LapceTerminal, LapceTerminalData, LapceTerminalView},
};
use std::{cmp::Ordering, sync::Arc};

use druid::{
    kurbo::{Line, Rect},
    piet::{PietTextLayout, Text, TextLayout, TextLayoutBuilder},
    widget::IdentityWrapper,
    Command, FontFamily, Target, WidgetId, WindowId,
};
use druid::{
    theme, BoxConstraints, Cursor, Data, Env, Event, EventCtx, LayoutCtx, LifeCycle,
    LifeCycleCtx, PaintCtx, Point, RenderContext, Size, UpdateCtx, Widget,
    WidgetExt, WidgetPod,
};
use lapce_proxy::terminal::TermId;
use strum::EnumMessage;

#[derive(Debug)]
pub enum SplitMoveDirection {
    Up,
    Down,
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

pub struct LapceSplitNew {
    split_id: WidgetId,
    children: Vec<ChildWidgetNew>,
    children_ids: Vec<WidgetId>,
    direction: SplitDirection,
    show_border: bool,
    commands: Vec<(LapceCommandNew, PietTextLayout, Rect, PietTextLayout)>,
}

pub struct ChildWidgetNew {
    pub widget: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
    flex: bool,
    params: f64,
    layout_rect: Rect,
}

impl LapceSplitNew {
    pub fn new(split_id: WidgetId) -> Self {
        Self {
            split_id,
            children: Vec::new(),
            children_ids: Vec::new(),
            direction: SplitDirection::Vertical,
            show_border: true,
            commands: vec![],
        }
    }

    pub fn direction(mut self, direction: SplitDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn horizontal(mut self) -> Self {
        self.direction = SplitDirection::Horizontal;
        self
    }

    pub fn hide_border(mut self) -> Self {
        self.show_border = false;
        self
    }

    pub fn with_flex_child(
        mut self,
        child: Box<dyn Widget<LapceTabData>>,
        child_id: Option<WidgetId>,
        params: f64,
    ) -> Self {
        let child = ChildWidgetNew {
            widget: WidgetPod::new(child),
            flex: true,
            params,
            layout_rect: Rect::ZERO,
        };
        self.children_ids
            .push(child_id.unwrap_or(child.widget.id()));
        self.children.push(child);
        self
    }

    pub fn with_child(
        mut self,
        child: Box<dyn Widget<LapceTabData>>,
        child_id: Option<WidgetId>,
        params: f64,
    ) -> Self {
        let child = ChildWidgetNew {
            widget: WidgetPod::new(child),
            flex: false,
            params,
            layout_rect: Rect::ZERO,
        };
        self.children_ids
            .push(child_id.unwrap_or(child.widget.id()));
        self.children.push(child);
        self
    }

    pub fn insert_flex_child(
        &mut self,
        index: usize,
        child: Box<dyn Widget<LapceTabData>>,
        child_id: Option<WidgetId>,
        params: f64,
    ) {
        let child = ChildWidgetNew {
            widget: WidgetPod::new(child),
            flex: true,
            params,
            layout_rect: Rect::ZERO,
        };
        self.children_ids
            .insert(index, child_id.unwrap_or(child.widget.id()));
        self.children.insert(index, child);
    }

    pub fn even_flex_children(&mut self) {
        for child in self.children.iter_mut() {
            if child.flex {
                child.params = 1.0;
            }
        }
    }

    fn paint_bar(&mut self, ctx: &mut PaintCtx, config: &Config) {
        let children_len = self.children.len();
        if children_len <= 1 {
            return;
        }

        let size = ctx.size();
        for i in 1..children_len {
            let line = if self.direction == SplitDirection::Vertical {
                let x = self.children[i].layout_rect.x0;
                let line = Line::new(Point::new(x, 0.0), Point::new(x, size.height));
                line
            } else {
                let y = self.children[i].layout_rect.y0;
                let line = Line::new(Point::new(0.0, y), Point::new(size.width, y));
                line
            };
            ctx.stroke(
                line,
                config.get_color_unchecked(LapceTheme::LAPCE_BORDER),
                1.0,
            );
        }
    }

    pub fn split_editor_close(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        widget_id: WidgetId,
    ) {
        if self.children.len() == 0 {
            return;
        }

        let mut index = 0;
        for (i, child_id) in self.children_ids.iter().enumerate() {
            if child_id == &widget_id {
                index = i;
                break;
            }
        }

        if self.children.len() > 1 {
            let new_index = if index >= self.children.len() - 1 {
                index - 1
            } else {
                index + 1
            };
            let new_view_id = self.children[new_index].widget.id();
            ctx.submit_command(Command::new(
                LAPCE_UI_COMMAND,
                LapceUICommand::Focus,
                Target::Widget(new_view_id),
            ));
        } else {
            data.main_split.active = Arc::new(None);
            ctx.submit_command(Command::new(
                LAPCE_UI_COMMAND,
                LapceUICommand::Focus,
                Target::Widget(self.split_id),
            ));
        }
        let view_id = self.children[index].widget.id();
        data.main_split.editors.remove(&view_id);
        self.children.remove(index);
        self.children_ids.remove(index);
        data.main_split.editors_order = Arc::new(self.children_ids.clone());

        self.even_flex_children();
        ctx.children_changed();
    }

    pub fn split_editor_exchange(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        widget_id: WidgetId,
    ) {
        if self.children.len() <= 1 {
            return;
        }

        let mut index = 0;
        for (i, child_id) in self.children_ids.iter().enumerate() {
            if child_id == &widget_id {
                index = i;
                break;
            }
        }
        if index >= self.children.len() - 1 {
            return;
        }

        let new_child = self.children_ids[index + 1];
        ctx.submit_command(Command::new(
            LAPCE_UI_COMMAND,
            LapceUICommand::Focus,
            Target::Widget(new_child),
        ));

        self.children.swap(index, index + 1);
        self.children_ids.swap(index, index + 1);

        if data.main_split.editors.contains_key(&widget_id) {
            data.main_split.editors_order = Arc::new(self.children_ids.clone());
        }

        ctx.request_layout();
    }

    pub fn split_editor_move(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        direction: &SplitMoveDirection,
        widget_id: WidgetId,
    ) {
        let mut index = 0;
        for (i, child_id) in self.children_ids.iter().enumerate() {
            if child_id == &widget_id {
                index = i;
                break;
            }
        }

        let new_index = if self.direction == SplitDirection::Vertical {
            match direction {
                SplitMoveDirection::Left => {
                    if index == 0 {
                        return;
                    }
                    index - 1
                }
                SplitMoveDirection::Right => {
                    if index >= self.children.len() - 1 {
                        return;
                    }
                    index + 1
                }
                _ => index,
            }
        } else {
            match direction {
                SplitMoveDirection::Up => {
                    if index == 0 {
                        return;
                    }
                    index - 1
                }
                SplitMoveDirection::Down => {
                    if index >= self.children.len() - 1 {
                        return;
                    }
                    index + 1
                }
                _ => index,
            }
        };

        if new_index != index {
            ctx.submit_command(Command::new(
                LAPCE_UI_COMMAND,
                LapceUICommand::Focus,
                Target::Widget(self.children_ids[new_index]),
            ));
            ctx.submit_command(Command::new(
                LAPCE_UI_COMMAND,
                LapceUICommand::EnsureCursorVisible(None),
                Target::Widget(self.children_ids[new_index]),
            ));
        }
    }

    pub fn split_terminal(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        vertical: bool,
        widget_id: WidgetId,
    ) {
        let mut index = 0;
        for (i, child_id) in self.children_ids.iter().enumerate() {
            if child_id == &widget_id {
                index = i;
                break;
            }
        }

        let terminal_data = Arc::new(LapceTerminalData::new(
            data.workspace.clone(),
            self.split_id,
            ctx.get_external_handle(),
            data.proxy.clone(),
        ));
        let terminal = LapceTerminalView::new(&terminal_data);
        Arc::make_mut(&mut data.terminal)
            .terminals
            .insert(terminal_data.term_id, terminal_data.clone());

        self.insert_flex_child(
            index + 1,
            terminal.boxed(),
            Some(terminal_data.widget_id),
            1.0,
        );
        self.even_flex_children();
        ctx.children_changed();
    }

    pub fn split_terminal_close(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        term_id: TermId,
        widget_id: WidgetId,
    ) {
        if self.children.len() == 0 {
            return;
        }

        if self.children.len() == 1 {
            Arc::make_mut(&mut data.terminal).terminals.remove(&term_id);
            self.children.remove(0);
            self.children_ids.remove(0);

            self.even_flex_children();
            ctx.children_changed();
            for (pos, panel) in data.panels.iter_mut() {
                if panel.active == PanelKind::Terminal {
                    Arc::make_mut(panel).shown = false;
                }
            }
            if let Some(active) = *data.main_split.active {
                ctx.submit_command(Command::new(
                    LAPCE_UI_COMMAND,
                    LapceUICommand::Focus,
                    Target::Widget(active),
                ));
            }
            return;
        }

        let mut index = 0;
        for (i, child_id) in self.children_ids.iter().enumerate() {
            if child_id == &widget_id {
                index = i;
                break;
            }
        }

        let new_index = if index >= self.children.len() - 1 {
            index - 1
        } else {
            index + 1
        };
        let terminal_id = self.children_ids[index];
        let new_terminal_id = self.children_ids[new_index];
        ctx.submit_command(Command::new(
            LAPCE_UI_COMMAND,
            LapceUICommand::Focus,
            Target::Widget(new_terminal_id),
        ));

        Arc::make_mut(&mut data.terminal).terminals.remove(&term_id);
        self.children.remove(index);
        self.children_ids.remove(index);

        self.even_flex_children();
        ctx.children_changed();
    }

    pub fn split_add_editor(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        widget_id: WidgetId,
    ) {
        let editor_data = data.main_split.editors.get(&widget_id).unwrap();
        let editor = LapceEditorView::new(&editor_data);
        self.insert_flex_child(0, editor.boxed(), Some(editor_data.view_id), 1.0);
        self.even_flex_children();
        ctx.children_changed();
        data.main_split.editors_order = Arc::new(self.children_ids.clone());

        ctx.submit_command(Command::new(
            LAPCE_UI_COMMAND,
            LapceUICommand::Focus,
            Target::Widget(widget_id),
        ));
    }

    pub fn split_editor(
        &mut self,
        ctx: &mut EventCtx,
        data: &mut LapceTabData,
        vertical: bool,
        widget_id: WidgetId,
    ) {
        let mut index = 0;
        for (i, child_id) in self.children_ids.iter().enumerate() {
            if child_id == &widget_id {
                index = i;
                break;
            }
        }

        let view_id = self.children[index].widget.id();
        let from_editor = data.main_split.editors.get(&view_id).unwrap();
        let mut editor_data = LapceEditorData::new(
            None,
            Some(self.split_id),
            from_editor.content.clone(),
            &data.config,
        );
        editor_data.cursor = from_editor.cursor.clone();
        editor_data.locations = from_editor.locations.clone();
        ctx.submit_command(Command::new(
            LAPCE_UI_COMMAND,
            LapceUICommand::ForceScrollTo(
                from_editor.scroll_offset.x,
                from_editor.scroll_offset.y,
            ),
            Target::Widget(editor_data.view_id),
        ));

        let editor = LapceEditorView::new(&editor_data);
        self.insert_flex_child(
            index + 1,
            editor.boxed(),
            Some(editor_data.view_id),
            1.0,
        );
        self.even_flex_children();
        ctx.children_changed();
        data.main_split
            .editors
            .insert(editor_data.view_id, Arc::new(editor_data));
        data.main_split.editors_order = Arc::new(self.children_ids.clone());
    }
}

impl Widget<LapceTabData> for LapceSplitNew {
    fn id(&self) -> Option<WidgetId> {
        Some(self.split_id)
    }

    fn event(
        &mut self,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut LapceTabData,
        env: &Env,
    ) {
        for child in self.children.iter_mut() {
            child.widget.event(ctx, event, data, env);
        }
        match event {
            Event::MouseMove(mouse_event) => {
                if self.children.len() == 0 {
                    let mut on_command = false;
                    for (_, _, rect, _) in &self.commands {
                        if rect.contains(mouse_event.pos) {
                            on_command = true;
                            break;
                        }
                    }
                    if on_command {
                        ctx.set_cursor(&druid::Cursor::Pointer);
                    } else {
                        ctx.clear_cursor();
                    }
                }
            }
            Event::MouseDown(mouse_event) => {
                if self.children.len() == 0 {
                    for (cmd, _, rect, _) in &self.commands {
                        if rect.contains(mouse_event.pos) {
                            ctx.submit_command(Command::new(
                                LAPCE_NEW_COMMAND,
                                cmd.clone(),
                                Target::Auto,
                            ));
                            return;
                        }
                    }
                }
            }
            Event::KeyDown(key_event) => {
                if self.children.len() == 0 {
                    ctx.set_handled();
                    let mut keypress = data.keypress.clone();
                    Arc::make_mut(&mut keypress).key_down(
                        ctx,
                        key_event,
                        &mut DefaultKeyPressHandler {},
                        env,
                    );
                }
            }
            Event::Command(cmd) if cmd.is(LAPCE_UI_COMMAND) => {
                let command = cmd.get_unchecked(LAPCE_UI_COMMAND);
                match command {
                    LapceUICommand::Focus => {
                        ctx.request_focus();
                        data.focus = self.split_id;
                        data.focus_area = FocusArea::Editor;
                    }
                    LapceUICommand::SplitAddEditor(widget_id) => {
                        self.split_add_editor(ctx, data, *widget_id);
                    }
                    LapceUICommand::SplitEditor(vertical, widget_id) => {
                        self.split_editor(ctx, data, *vertical, *widget_id);
                    }
                    LapceUICommand::SplitEditorMove(direction, widget_id) => {
                        self.split_editor_move(ctx, data, direction, *widget_id);
                    }
                    LapceUICommand::SplitEditorExchange(widget_id) => {
                        self.split_editor_exchange(ctx, data, *widget_id);
                    }
                    LapceUICommand::SplitEditorClose(widget_id) => {
                        self.split_editor_close(ctx, data, *widget_id);
                    }
                    LapceUICommand::SplitTerminal(vertical, widget_id) => {
                        self.split_terminal(ctx, data, *vertical, *widget_id);
                    }
                    LapceUICommand::SplitTerminalClose(term_id, widget_id) => {
                        self.split_terminal_close(ctx, data, *term_id, *widget_id);
                    }
                    LapceUICommand::InitTerminalPanel(focus) => {
                        if data.terminal.terminals.len() == 0 {
                            let terminal_data = Arc::new(LapceTerminalData::new(
                                data.workspace.clone(),
                                data.terminal.split_id,
                                ctx.get_external_handle(),
                                data.proxy.clone(),
                            ));
                            let terminal = LapceTerminalView::new(&terminal_data);
                            self.insert_flex_child(
                                0,
                                terminal.boxed(),
                                Some(terminal_data.widget_id),
                                1.0,
                            );
                            if *focus {
                                ctx.submit_command(Command::new(
                                    LAPCE_UI_COMMAND,
                                    LapceUICommand::Focus,
                                    Target::Widget(terminal_data.widget_id),
                                ));
                            }
                            let terminal_panel = Arc::make_mut(&mut data.terminal);
                            terminal_panel.active = terminal_data.widget_id;
                            terminal_panel.active_term_id = terminal_data.term_id;
                            terminal_panel
                                .terminals
                                .insert(terminal_data.term_id, terminal_data);
                            ctx.children_changed();
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &LapceTabData,
        env: &Env,
    ) {
        for child in self.children.iter_mut() {
            child.widget.lifecycle(ctx, event, data, env);
        }
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &LapceTabData,
        data: &LapceTabData,
        env: &Env,
    ) {
        for child in self.children.iter_mut() {
            child.widget.update(ctx, data, env);
        }
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        data: &LapceTabData,
        env: &Env,
    ) -> Size {
        let my_size = bc.max();

        let children_len = self.children.len();
        if children_len == 0 {
            let origin =
                Point::new(my_size.width / 2.0, my_size.height / 2.0 + 40.0);
            let line_height = data.config.editor.line_height as f64;

            self.commands = empty_editor_commands(
                data.config.lapce.modal,
                data.workspace.path.is_some(),
            )
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let text_layout = ctx
                    .text()
                    .new_text_layout(cmd.palette_desc.as_ref().unwrap().to_string())
                    .font(FontFamily::SYSTEM_UI, 14.0)
                    .text_color(
                        data.config
                            .get_color_unchecked(LapceTheme::EDITOR_DIM)
                            .clone(),
                    )
                    .build()
                    .unwrap();
                let point =
                    origin - (text_layout.size().width, -line_height * i as f64);
                let rect = text_layout.size().to_rect().with_origin(point);
                let mut key = None;
                for (_, keymaps) in data.keypress.keymaps.iter() {
                    for keymap in keymaps {
                        if keymap.command == cmd.cmd {
                            let mut keymap_str = "".to_string();
                            for keypress in &keymap.key {
                                if keymap_str != "" {
                                    keymap_str += " "
                                }
                                keymap_str += &keybinding_to_string(keypress);
                            }
                            key = Some(keymap_str);
                            break;
                        }
                    }
                    if key.is_some() {
                        break;
                    }
                }
                let key_text_layout = ctx
                    .text()
                    .new_text_layout(key.unwrap_or("Unbound".to_string()))
                    .font(FontFamily::SYSTEM_UI, 14.0)
                    .text_color(
                        data.config
                            .get_color_unchecked(LapceTheme::EDITOR_DIM)
                            .clone(),
                    )
                    .build()
                    .unwrap();
                (cmd.clone(), text_layout, rect, key_text_layout)
            })
            .collect();
            return my_size;
        }

        let mut non_flex_total = 0.0;
        let mut max_other_axis = 0.0;
        for child in self.children.iter_mut() {
            if !child.flex {
                let (width, height) = match self.direction {
                    SplitDirection::Vertical => (child.params, my_size.height),
                    SplitDirection::Horizontal => (my_size.width, child.params),
                };
                let size = Size::new(width, height);
                let size = child.widget.layout(
                    ctx,
                    &BoxConstraints::new(Size::ZERO, size),
                    data,
                    env,
                );
                non_flex_total += match self.direction {
                    SplitDirection::Vertical => size.width,
                    SplitDirection::Horizontal => size.height,
                };
                match self.direction {
                    SplitDirection::Vertical => {
                        if size.height > max_other_axis {
                            max_other_axis = size.height;
                        }
                    }
                    SplitDirection::Horizontal => {
                        if size.width > max_other_axis {
                            max_other_axis = size.width;
                        }
                    }
                }
                child.layout_rect = child.layout_rect.with_size(size)
            };
        }

        let mut flex_sum = 0.0;
        for child in &self.children {
            if child.flex {
                flex_sum += child.params;
            }
        }

        let flex_total = if self.direction == SplitDirection::Vertical {
            my_size.width
        } else {
            my_size.height
        } - non_flex_total;

        let mut x = 0.0;
        let mut y = 0.0;
        for child in self.children.iter_mut() {
            if !child.flex {
                child.widget.set_origin(ctx, data, env, Point::new(x, y));
                child.layout_rect = child.layout_rect.with_origin(Point::new(x, y));
            } else {
                let flex = flex_total / flex_sum * child.params;
                let (width, height) = match self.direction {
                    SplitDirection::Vertical => (flex, my_size.height),
                    SplitDirection::Horizontal => (my_size.width, flex),
                };
                let size = Size::new(width, height);
                let size = child.widget.layout(
                    ctx,
                    &BoxConstraints::new(Size::ZERO, size),
                    data,
                    env,
                );
                match self.direction {
                    SplitDirection::Vertical => {
                        if size.height > max_other_axis {
                            max_other_axis = size.height;
                        }
                    }
                    SplitDirection::Horizontal => {
                        if size.width > max_other_axis {
                            max_other_axis = size.width;
                        }
                    }
                }
                child.widget.set_origin(ctx, data, env, Point::new(x, y));
                child.layout_rect = child
                    .layout_rect
                    .with_origin(Point::new(x, y))
                    .with_size(size);
            }
            match self.direction {
                SplitDirection::Vertical => x += child.layout_rect.size().width,
                SplitDirection::Horizontal => y += child.layout_rect.size().height,
            }
        }

        match self.direction {
            SplitDirection::Vertical => Size::new(x, max_other_axis),
            SplitDirection::Horizontal => Size::new(max_other_axis, y),
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &LapceTabData, env: &Env) {
        if self.children.len() == 0 {
            let rect = ctx.size().to_rect();
            ctx.fill(
                rect,
                data.config
                    .get_color_unchecked(LapceTheme::EDITOR_BACKGROUND),
            );
            ctx.with_save(|ctx| {
                ctx.clip(rect);
                let svg = logo_svg();
                let size = ctx.size();
                let svg_size = 100.0;
                let rect = Size::ZERO
                    .to_rect()
                    .with_origin(
                        Point::new(size.width / 2.0, size.height / 2.0)
                            + (0.0, -svg_size),
                    )
                    .inflate(svg_size, svg_size);
                ctx.draw_svg(
                    &svg,
                    rect,
                    Some(
                        &data
                            .config
                            .get_color_unchecked(LapceTheme::EDITOR_DIM)
                            .clone()
                            .with_alpha(0.5),
                    ),
                );

                for (cmd, text, rect, keymap) in &self.commands {
                    ctx.draw_text(text, rect.origin());
                    ctx.draw_text(
                        keymap,
                        rect.origin() + (20.0 + rect.width(), 0.0),
                    );
                }
            });

            return;
        }
        for child in self.children.iter_mut() {
            child.widget.paint(ctx, data, env);
        }
        if self.show_border {
            self.paint_bar(ctx, &data.config);
        }
    }
}

fn empty_editor_commands(modal: bool, has_workspace: bool) -> Vec<LapceCommandNew> {
    if !has_workspace {
        vec![
            LapceCommandNew {
                cmd: LapceWorkbenchCommand::PaletteCommand.to_string(),
                data: None,
                palette_desc: Some("Show All Commands".to_string()),
                target: CommandTarget::Workbench,
            },
            if modal {
                LapceCommandNew {
                    cmd: LapceWorkbenchCommand::DisableModal.to_string(),
                    data: None,
                    palette_desc: LapceWorkbenchCommand::DisableModal
                        .get_message()
                        .map(|m| m.to_string()),
                    target: CommandTarget::Workbench,
                }
            } else {
                LapceCommandNew {
                    cmd: LapceWorkbenchCommand::EnableModal.to_string(),
                    data: None,
                    palette_desc: LapceWorkbenchCommand::EnableModal
                        .get_message()
                        .map(|m| m.to_string()),
                    target: CommandTarget::Workbench,
                }
            },
            LapceCommandNew {
                cmd: LapceWorkbenchCommand::OpenFolder.to_string(),
                data: None,
                palette_desc: Some("Open Folder".to_string()),
                target: CommandTarget::Workbench,
            },
            LapceCommandNew {
                cmd: LapceWorkbenchCommand::PaletteWorkspace.to_string(),
                data: None,
                palette_desc: Some("Open Recent".to_string()),
                target: CommandTarget::Workbench,
            },
        ]
    } else {
        vec![
            LapceCommandNew {
                cmd: LapceWorkbenchCommand::PaletteCommand.to_string(),
                data: None,
                palette_desc: Some("Show All Commands".to_string()),
                target: CommandTarget::Workbench,
            },
            if modal {
                LapceCommandNew {
                    cmd: LapceWorkbenchCommand::DisableModal.to_string(),
                    data: None,
                    palette_desc: LapceWorkbenchCommand::DisableModal
                        .get_message()
                        .map(|m| m.to_string()),
                    target: CommandTarget::Workbench,
                }
            } else {
                LapceCommandNew {
                    cmd: LapceWorkbenchCommand::EnableModal.to_string(),
                    data: None,
                    palette_desc: LapceWorkbenchCommand::EnableModal
                        .get_message()
                        .map(|m| m.to_string()),
                    target: CommandTarget::Workbench,
                }
            },
            LapceCommandNew {
                cmd: LapceWorkbenchCommand::Palette.to_string(),
                data: None,
                palette_desc: Some("Go To File".to_string()),
                target: CommandTarget::Workbench,
            },
        ]
    }
}

fn keybinding_to_string(keypress: &KeyPress) -> String {
    let mut keymap_str = "".to_string();
    if keypress.mods.ctrl() {
        keymap_str += "Ctrl+";
    }
    if keypress.mods.alt() {
        keymap_str += "Alt+";
    }
    if keypress.mods.meta() {
        let keyname = match std::env::consts::OS {
            "macos" => "Cmd",
            "windows" => "Win",
            _ => "Meta",
        };
        keymap_str += &keyname;
        keymap_str += "+";
    }
    if keypress.mods.shift() {
        keymap_str += "Shift+";
    }
    keymap_str += &keypress.key.to_string();
    keymap_str
}
