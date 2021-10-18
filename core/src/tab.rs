use std::{collections::HashMap, path::PathBuf, sync::Arc, thread};

use directories::ProjectDirs;
use druid::{
    kurbo::Line,
    piet::{Text, TextLayout, TextLayoutBuilder},
    theme, BoxConstraints, Color, Command, Cursor, Data, Env, Event, EventCtx,
    FontFamily, Insets, LayoutCtx, LifeCycle, LifeCycleCtx, PaintCtx, Point, Rect,
    RenderContext, Size, Target, Vec2, Widget, WidgetExt, WidgetId, WidgetPod,
    WindowConfig,
};
use lsp_types::{CallHierarchyOptions, DiagnosticSeverity};

use crate::{
    buffer::{BufferId, BufferNew, BufferState, BufferUpdate, UpdateEvent},
    code_action::CodeAction,
    command::{
        LapceCommand, LapceUICommand, LAPCE_COMMAND, LAPCE_NEW_COMMAND,
        LAPCE_UI_COMMAND,
    },
    completion::{CompletionContainer, CompletionNew, CompletionStatus},
    config::{Config, LapceTheme},
    data::{
        EditorContent, EditorDiagnostic, EditorKind, LapceMainSplitData,
        LapceTabData,
    },
    editor::{EditorLocationNew, LapceEditorView},
    movement::{self, CursorMode, Selection},
    palette::{NewPalette, PaletteViewLens},
    panel::{PanelPosition, PanelResizePosition},
    scroll::LapceScrollNew,
    source_control::SourceControlNew,
    split::LapceSplitNew,
    state::{LapceWorkspace, LapceWorkspaceType},
    status::LapceStatusNew,
    terminal::TerminalPanel,
};

pub struct LapceTabNew {
    id: WidgetId,
    main_split: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
    completion: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
    palette: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
    code_action: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
    status: WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>,
    panels:
        HashMap<WidgetId, WidgetPod<LapceTabData, Box<dyn Widget<LapceTabData>>>>,
    current_bar_hover: Option<PanelResizePosition>,
    height: f64,
    main_split_height: f64,
    status_height: f64,
}

impl LapceTabNew {
    pub fn new(data: &LapceTabData) -> Self {
        let editor = data.main_split.active_editor();
        let main_split = LapceSplitNew::new(*data.main_split.split_id)
            .with_flex_child(
                LapceEditorView::new(editor).boxed(),
                Some(editor.view_id),
                1.0,
            );
        let completion = CompletionContainer::new(&data.completion);
        let palette = NewPalette::new(
            &data.palette,
            data.main_split
                .editors
                .get(&data.palette.preview_editor)
                .unwrap(),
        );
        let status = LapceStatusNew::new();
        let code_action = CodeAction::new();

        let mut panels = HashMap::new();
        let source_control = SourceControlNew::new(&data);
        panels.insert(
            data.source_control.widget_id,
            WidgetPod::new(source_control.boxed()),
        );
        let terminal = TerminalPanel::new(&data);
        panels.insert(data.terminal.widget_id, WidgetPod::new(terminal.boxed()));

        Self {
            id: data.id,
            main_split: WidgetPod::new(main_split.boxed()),
            completion: WidgetPod::new(completion.boxed()),
            code_action: WidgetPod::new(code_action.boxed()),
            palette: WidgetPod::new(palette.boxed()),
            status: WidgetPod::new(status.boxed()),
            panels,
            current_bar_hover: None,
            height: 0.0,
            main_split_height: 0.0,
            status_height: 0.0,
        }
    }

    fn update_split_point(&mut self, data: &mut LapceTabData, mouse_pos: Point) {
        if let Some(position) = self.current_bar_hover.as_ref() {
            match position {
                PanelResizePosition::Left => {
                    data.panel_size.left = mouse_pos.x.round().max(50.0);
                }
                PanelResizePosition::LeftSplit => (),
                PanelResizePosition::Bottom => {
                    data.panel_size.bottom =
                        (self.height - mouse_pos.y.round() - self.status_height)
                            .max(50.0);
                }
            }
        }
    }

    fn bar_hit_test(
        &self,
        data: &LapceTabData,
        mouse_pos: Point,
    ) -> Option<PanelResizePosition> {
        let panel_left_top_shown = data
            .panels
            .get(&PanelPosition::LeftTop)
            .map(|p| p.is_shown())
            .unwrap_or(false);
        let panel_left_bottom_shown = data
            .panels
            .get(&PanelPosition::LeftBottom)
            .map(|p| p.is_shown())
            .unwrap_or(false);
        let left = if panel_left_bottom_shown || panel_left_top_shown {
            let left = data.panel_size.left;
            if mouse_pos.x >= left - 3.0 && mouse_pos.x <= left + 3.0 {
                return Some(PanelResizePosition::Left);
            }
            left
        } else {
            0.0
        };

        let panel_bottom_left_shown = data
            .panels
            .get(&PanelPosition::BottomLeft)
            .map(|p| p.is_shown())
            .unwrap_or(false);
        let panel_bottom_right_shown = data
            .panels
            .get(&PanelPosition::BottomRight)
            .map(|p| p.is_shown())
            .unwrap_or(false);
        if panel_bottom_left_shown || panel_bottom_right_shown {
            let bottom = data.panel_size.bottom;
            let y = self.main_split_height;
            if mouse_pos.x > left && mouse_pos.y >= y - 3.0 && mouse_pos.y <= y + 3.0
            {
                return Some(PanelResizePosition::Bottom);
            }
        }

        None
    }
}

impl Widget<LapceTabData> for LapceTabNew {
    fn id(&self) -> Option<WidgetId> {
        Some(self.id)
    }

    fn event(
        &mut self,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut LapceTabData,
        env: &Env,
    ) {
        match event {
            Event::MouseDown(mouse) => {
                if mouse.button.is_left() {
                    if let Some(position) = self.bar_hit_test(data, mouse.pos) {
                        self.current_bar_hover = Some(position);
                        ctx.set_active(true);
                        ctx.set_handled();
                    }
                }
            }
            Event::MouseUp(mouse) => {
                if mouse.button.is_left() && ctx.is_active() {
                    ctx.set_active(false);
                }
            }
            Event::MouseMove(mouse) => {
                if ctx.is_active() {
                    self.update_split_point(data, mouse.pos);
                    ctx.request_layout();
                    ctx.set_handled();
                } else {
                    match self.bar_hit_test(data, mouse.pos) {
                        Some(PanelResizePosition::Left) => {
                            ctx.set_cursor(&Cursor::ResizeLeftRight)
                        }
                        Some(PanelResizePosition::LeftSplit) => {
                            ctx.set_cursor(&Cursor::ResizeUpDown)
                        }
                        Some(PanelResizePosition::Bottom) => {
                            ctx.set_cursor(&Cursor::ResizeUpDown)
                        }
                        None => ctx.clear_cursor(),
                    }
                }
            }
            Event::Command(cmd) if cmd.is(LAPCE_NEW_COMMAND) => {
                let command = cmd.get_unchecked(LAPCE_NEW_COMMAND);
                data.run_command(ctx, command, None, env);
                ctx.set_handled();
            }
            Event::Command(cmd) if cmd.is(LAPCE_UI_COMMAND) => {
                let command = cmd.get_unchecked(LAPCE_UI_COMMAND);
                match command {
                    LapceUICommand::UpdateWindowOrigin => {
                        let window_origin = ctx.window_origin();
                        if data.window_origin != window_origin {
                            data.window_origin = window_origin;
                        }
                    }
                    LapceUICommand::LoadBuffer { path, content } => {
                        let buffer =
                            data.main_split.open_files.get_mut(path).unwrap();
                        Arc::make_mut(buffer).load_content(content);
                        ctx.set_handled();
                    }
                    LapceUICommand::TerminalUpdateContent(id, content) => {
                        if let Some(terminal) =
                            Arc::make_mut(&mut data.terminal).terminals.get_mut(id)
                        {
                            let terminal = Arc::make_mut(terminal);
                            terminal.content = content.to_owned();
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::CloseTerminal(id) => {
                        let terminal_panel = Arc::make_mut(&mut data.terminal);
                        if let Some(terminal) = terminal_panel.terminals.get_mut(id)
                        {
                            ctx.submit_command(Command::new(
                                LAPCE_UI_COMMAND,
                                LapceUICommand::SplitTerminalClose(
                                    terminal.term_id,
                                    terminal.widget_id,
                                    terminal.panel_widget_id.clone(),
                                ),
                                Target::Widget(terminal.split_id),
                            ));
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::UpdateDiffFiles(files) => {
                        let source_control = Arc::make_mut(&mut data.source_control);
                        source_control.diff_files = files
                            .iter()
                            .map(|path| {
                                let mut checked = true;
                                for (p, c) in source_control.diff_files.iter() {
                                    if p == path {
                                        checked = *c;
                                        break;
                                    }
                                }
                                (path.clone(), checked)
                            })
                            .collect();
                        ctx.set_handled();
                    }
                    LapceUICommand::PublishDiagnostics(diagnostics) => {
                        let path = PathBuf::from(diagnostics.uri.path());
                        let diagnostics = diagnostics
                            .diagnostics
                            .iter()
                            .map(|d| EditorDiagnostic {
                                range: None,
                                diagnositc: d.clone(),
                            })
                            .collect();
                        data.main_split
                            .diagnostics
                            .insert(path, Arc::new(diagnostics));

                        let mut errors = 0;
                        let mut warnings = 0;
                        for (_, diagnositics) in data.main_split.diagnostics.iter() {
                            for diagnositic in diagnositics.iter() {
                                if let Some(severity) =
                                    diagnositic.diagnositc.severity
                                {
                                    match severity {
                                        DiagnosticSeverity::Error => errors += 1,
                                        DiagnosticSeverity::Warning => warnings += 1,
                                        _ => (),
                                    }
                                }
                            }
                        }
                        data.main_split.error_count = errors;
                        data.main_split.warning_count = warnings;

                        ctx.set_handled();
                    }
                    LapceUICommand::DocumentFormatAndSave(path, rev, result) => {
                        data.main_split
                            .document_format_and_save(ctx, path, *rev, result);
                        ctx.set_handled();
                    }
                    LapceUICommand::BufferSave(path, rev) => {
                        let buffer =
                            data.main_split.open_files.get_mut(path).unwrap();
                        if buffer.rev == *rev {
                            Arc::make_mut(buffer).dirty = false;
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::LoadBufferAndGoToPosition {
                        path,
                        content,
                        editor_view_id,
                        location,
                    } => {
                        let buffer =
                            data.main_split.open_files.get_mut(path).unwrap();
                        Arc::make_mut(buffer).load_content(content);
                        data.main_split.go_to_location(
                            ctx,
                            *editor_view_id,
                            location.clone(),
                            &data.config,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::OpenFile(path) => {
                        let editor_view_id = data.main_split.active.clone();
                        data.main_split.jump_to_location(
                            ctx,
                            *editor_view_id,
                            EditorLocationNew {
                                path: path.clone(),
                                position: None,
                                scroll_offset: None,
                            },
                            &data.config,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::GoToLocationNew(editor_view_id, location) => {
                        data.main_split.go_to_location(
                            ctx,
                            *editor_view_id,
                            location.clone(),
                            &data.config,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::JumpToPosition(kind, position) => {
                        let editor_view_id =
                            data.main_split.editor_kind(kind).view_id;
                        data.main_split.jump_to_position(
                            ctx,
                            editor_view_id,
                            *position,
                            &data.config,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::JumpToLocation(kind, location) => {
                        let editor_view_id =
                            data.main_split.editor_kind(kind).view_id;
                        data.main_split.jump_to_location(
                            ctx,
                            editor_view_id,
                            location.clone(),
                            &data.config,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::JumpToLine(kind, line) => {
                        let editor_view_id =
                            data.main_split.editor_kind(kind).view_id;
                        data.main_split.jump_to_line(
                            ctx,
                            editor_view_id,
                            *line,
                            &data.config,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::GotoDefinition(
                        editor_view_id,
                        offset,
                        location,
                    ) => {
                        if *offset == data.main_split.active_editor().cursor.offset()
                        {
                            data.main_split.jump_to_location(
                                ctx,
                                *editor_view_id,
                                location.clone(),
                                &data.config,
                            );
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::GotoReference(
                        editor_view_id,
                        offset,
                        location,
                    ) => {
                        if *offset == data.main_split.active_editor().cursor.offset()
                        {
                            data.main_split.jump_to_location(
                                ctx,
                                *editor_view_id,
                                location.clone(),
                                &data.config,
                            );
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::UpdateCodeActions(path, rev, offset, resp) => {
                        if let Some(buffer) =
                            data.main_split.open_files.get_mut(path)
                        {
                            if buffer.rev == *rev {
                                Arc::make_mut(buffer)
                                    .code_actions
                                    .insert(*offset, resp.clone());
                            }
                        }
                    }
                    LapceUICommand::PaletteReferences(offset, locations) => {
                        if *offset == data.main_split.active_editor().cursor.offset()
                        {
                            let locations = locations
                                .iter()
                                .map(|l| EditorLocationNew {
                                    path: PathBuf::from(l.uri.path()),
                                    position: Some(l.range.start.clone()),
                                    scroll_offset: None,
                                })
                                .collect();
                            ctx.submit_command(Command::new(
                                LAPCE_UI_COMMAND,
                                LapceUICommand::RunPaletteReferences(locations),
                                Target::Widget(data.palette.widget_id),
                            ));
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::ReloadBuffer(id, rev, new_content) => {
                        for (_, buffer) in data.main_split.open_files.iter_mut() {
                            if &buffer.id == id {
                                if buffer.rev + 1 == *rev {
                                    let buffer = Arc::make_mut(buffer);
                                    buffer.load_content(new_content);
                                    buffer.rev = *rev;

                                    for (_, editor) in
                                        data.main_split.editors.iter_mut()
                                    {
                                        match &editor.content {
                                            EditorContent::Buffer(path) => {
                                                if path == &buffer.path {
                                                    if editor.cursor.offset()
                                                        >= buffer.len()
                                                    {
                                                        let editor =
                                                            Arc::make_mut(editor);
                                                        if data.config.lapce.modal {
                                                            editor.cursor =
                                                                movement::Cursor::new(CursorMode::Normal(buffer.len() - 1), None);
                                                        } else {
                                                            editor.cursor =
                                                                movement::Cursor::new(CursorMode::Insert(Selection::caret(buffer.len() - 1)), None);
                                                        }
                                                    }
                                                }
                                            }
                                            EditorContent::None => {}
                                        }
                                    }
                                }
                                break;
                            }
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::UpdateBufferLineChanges(
                        id,
                        rev,
                        line_changes,
                    ) => {
                        for (_, buffer) in data.main_split.open_files.iter_mut() {
                            if &buffer.id == id {
                                if buffer.rev == *rev {
                                    let buffer = Arc::make_mut(buffer);
                                    buffer.line_changes = line_changes.to_owned();
                                }
                                break;
                            }
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::UpdateSemanticTokens(id, path, rev, tokens) => {
                        let buffer =
                            data.main_split.open_files.get_mut(path).unwrap();
                        if buffer.rev == *rev {
                            if let Some(language) = buffer.language.as_ref() {
                                data.update_sender.send(
                                    UpdateEvent::SemanticTokens(
                                        BufferUpdate {
                                            id: buffer.id,
                                            path: buffer.path.clone(),
                                            rope: buffer.rope.clone(),
                                            rev: *rev,
                                            language: *language,
                                            highlights: buffer.styles.clone(),
                                            semantic_tokens: true,
                                        },
                                        tokens.to_owned(),
                                    ),
                                );
                            }
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::ShowCodeActions
                    | LapceUICommand::CancelCodeActions => {
                        self.code_action.event(ctx, event, data, env);
                    }
                    LapceUICommand::FocusTab => {
                        let dir = data
                            .workspace
                            .as_ref()
                            .map(|w| {
                                let dir =
                                    w.path.file_name().unwrap().to_str().unwrap();
                                let dir = match &w.kind {
                                    LapceWorkspaceType::Local => dir.to_string(),
                                    LapceWorkspaceType::RemoteSSH(user, host) => {
                                        format!("{} [{}@{}]", dir, user, host)
                                    }
                                };
                                dir
                            })
                            .unwrap_or("Lapce".to_string());
                        ctx.configure_window(WindowConfig::default().set_title(dir));
                        ctx.submit_command(Command::new(
                            LAPCE_UI_COMMAND,
                            LapceUICommand::Focus,
                            Target::Widget(data.focus),
                        ));
                        ctx.set_handled();
                    }
                    LapceUICommand::UpdateStyle {
                        id,
                        path,
                        rev,
                        highlights,
                        semantic_tokens,
                    } => {
                        let buffer =
                            data.main_split.open_files.get_mut(path).unwrap();
                        Arc::make_mut(buffer).update_styles(
                            *rev,
                            highlights.to_owned(),
                            *semantic_tokens,
                        );
                        ctx.set_handled();
                    }
                    LapceUICommand::FocusSourceControl => {
                        for (_, panel) in data.panels.iter_mut() {
                            for widget_id in panel.widgets.clone() {
                                if widget_id == data.source_control.widget_id {
                                    let panel = Arc::make_mut(panel);
                                    panel.active = widget_id;
                                    panel.shown = true;
                                    ctx.submit_command(Command::new(
                                        LAPCE_UI_COMMAND,
                                        LapceUICommand::Focus,
                                        Target::Widget(widget_id),
                                    ));
                                }
                            }
                        }
                        ctx.set_handled();
                    }
                    LapceUICommand::FocusEditor => {
                        ctx.submit_command(Command::new(
                            LAPCE_UI_COMMAND,
                            LapceUICommand::Focus,
                            Target::Widget(*data.main_split.active),
                        ));
                        ctx.set_handled();
                    }
                    LapceUICommand::UpdateSyntaxTree {
                        id,
                        path,
                        rev,
                        tree,
                    } => {
                        let buffer =
                            data.main_split.open_files.get_mut(path).unwrap();
                        Arc::make_mut(buffer)
                            .update_syntax_tree(*rev, tree.to_owned());
                        ctx.set_handled();
                    }
                    _ => (),
                }
            }
            _ => (),
        }
        self.palette.event(ctx, event, data, env);
        self.completion.event(ctx, event, data, env);
        self.code_action.event(ctx, event, data, env);
        self.main_split.event(ctx, event, data, env);
        self.status.event(ctx, event, data, env);
        for (_, panel) in data.panels.clone().iter() {
            if panel.is_shown() {
                self.panels
                    .get_mut(&panel.active)
                    .unwrap()
                    .event(ctx, event, data, env);
            }
        }
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &LapceTabData,
        env: &Env,
    ) {
        match event {
            LifeCycle::WidgetAdded => {
                data.proxy.start(
                    data.workspace
                        .clone()
                        .map(|w| (*w).clone())
                        .unwrap_or(LapceWorkspace::default()),
                    ctx.get_external_handle(),
                );
            }
            _ => {}
        }

        self.palette.lifecycle(ctx, event, data, env);
        self.main_split.lifecycle(ctx, event, data, env);
        self.code_action.lifecycle(ctx, event, data, env);
        self.status.lifecycle(ctx, event, data, env);
        self.completion.lifecycle(ctx, event, data, env);

        for (_, panel) in self.panels.iter_mut() {
            panel.lifecycle(ctx, event, data, env);
        }
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &LapceTabData,
        data: &LapceTabData,
        env: &Env,
    ) {
        if old_data.focus != data.focus {
            ctx.request_paint();
        }

        if old_data
            .main_split
            .diagnostics
            .same(&data.main_split.diagnostics)
        {
            ctx.request_paint();
        }

        if !old_data.panels.same(&data.panels) {
            ctx.request_layout();
        }

        if !old_data.config.same(&data.config) {
            ctx.request_layout();
        }

        self.palette.update(ctx, data, env);
        self.main_split.update(ctx, data, env);
        self.completion.update(ctx, data, env);
        self.code_action.update(ctx, data, env);
        self.status.update(ctx, data, env);
        for (_, panel) in data.panels.iter() {
            if panel.is_shown() {
                self.panels
                    .get_mut(&panel.active)
                    .unwrap()
                    .update(ctx, data, env);
            }
        }
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        data: &LapceTabData,
        env: &Env,
    ) -> Size {
        // ctx.set_paint_insets((0.0, 10.0, 0.0, 0.0));
        let self_size = bc.max();
        self.height = self_size.height;

        let status_size = self.status.layout(ctx, bc, data, env);
        self.status.set_origin(
            ctx,
            data,
            env,
            Point::new(0.0, self_size.height - status_size.height),
        );
        self.status_height = status_size.height;

        let panel_left_top_shown = data
            .panels
            .get(&PanelPosition::LeftTop)
            .map(|p| p.is_shown())
            .unwrap_or(false);
        let panel_left_bottom_shown = data
            .panels
            .get(&PanelPosition::LeftBottom)
            .map(|p| p.is_shown())
            .unwrap_or(false);
        let panel_left_width = if panel_left_top_shown || panel_left_bottom_shown {
            let left_width = data.panel_size.left;
            if panel_left_top_shown && panel_left_bottom_shown {
                let top_height = (self_size.height - status_size.height)
                    * data.panel_size.left_split;
                let bottom_height =
                    self_size.height - status_size.height - top_height;

                let panel_left_top = self
                    .panels
                    .get_mut(
                        &data.panels.get(&PanelPosition::LeftTop).unwrap().active,
                    )
                    .unwrap();
                panel_left_top.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(left_width, top_height)),
                    data,
                    env,
                );
                panel_left_top.set_origin(ctx, data, env, Point::ZERO);

                let panel_left_bottom = self
                    .panels
                    .get_mut(
                        &data.panels.get(&PanelPosition::LeftBottom).unwrap().active,
                    )
                    .unwrap();
                panel_left_bottom.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(left_width, bottom_height)),
                    data,
                    env,
                );
                panel_left_bottom.set_origin(
                    ctx,
                    data,
                    env,
                    Point::new(0.0, top_height),
                );
            } else if panel_left_top_shown {
                let top_height = self_size.height - status_size.height;
                let panel_left_top = self
                    .panels
                    .get_mut(
                        &data.panels.get(&PanelPosition::LeftTop).unwrap().active,
                    )
                    .unwrap();
                panel_left_top.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(left_width, top_height)),
                    data,
                    env,
                );
                panel_left_top.set_origin(ctx, data, env, Point::ZERO);
            } else if panel_left_bottom_shown {
                let bottom_height = self_size.height - status_size.height;
                let panel_left_bottom = self
                    .panels
                    .get_mut(
                        &data.panels.get(&PanelPosition::LeftBottom).unwrap().active,
                    )
                    .unwrap();
                panel_left_bottom.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(left_width, bottom_height)),
                    data,
                    env,
                );
                panel_left_bottom.set_origin(ctx, data, env, Point::ZERO);
            }
            left_width
        } else {
            0.0
        };

        let (panel_bottom_left_shown, panel_bottom_left_maximized) = data
            .panels
            .get(&PanelPosition::BottomLeft)
            .map(|p| (p.is_shown(), p.is_maximized()))
            .unwrap_or((false, false));
        let (panel_bottom_right_shown, panel_bottom_right_maximized) = data
            .panels
            .get(&PanelPosition::BottomRight)
            .map(|p| (p.is_shown(), p.is_maximized()))
            .unwrap_or((false, false));
        let panel_bottom_height = if panel_bottom_left_shown
            || panel_bottom_right_shown
        {
            let maximized =
                panel_bottom_left_maximized || panel_bottom_right_maximized;
            let bottom_height = if maximized {
                self_size.height - status_size.height
            } else {
                data.panel_size.bottom
            };
            let panel_x = panel_left_width;
            let panel_y = self_size.height - status_size.height - bottom_height;
            let panel_width = self_size.width - panel_left_width;
            if panel_bottom_left_shown && panel_bottom_right_shown {
                let left_width = panel_width * data.panel_size.bottom_split;
                let right_width = panel_width - left_width;

                let panel_bottom_left = self
                    .panels
                    .get_mut(
                        &data.panels.get(&PanelPosition::BottomLeft).unwrap().active,
                    )
                    .unwrap();
                panel_bottom_left.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(left_width, bottom_height)),
                    data,
                    env,
                );
                panel_bottom_left.set_origin(
                    ctx,
                    data,
                    env,
                    Point::new(panel_left_width, panel_y),
                );

                let panel_bottom_right = self
                    .panels
                    .get_mut(
                        &data
                            .panels
                            .get(&PanelPosition::BottomRight)
                            .unwrap()
                            .active,
                    )
                    .unwrap();
                panel_bottom_right.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(right_width, bottom_height)),
                    data,
                    env,
                );
                panel_bottom_right.set_origin(
                    ctx,
                    data,
                    env,
                    Point::new(panel_left_width + left_width, panel_y),
                );
            } else if panel_bottom_left_shown {
                let panel_bottom_left = self
                    .panels
                    .get_mut(
                        &data.panels.get(&PanelPosition::BottomLeft).unwrap().active,
                    )
                    .unwrap();
                panel_bottom_left.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(panel_width, bottom_height)),
                    data,
                    env,
                );
                panel_bottom_left.set_origin(
                    ctx,
                    data,
                    env,
                    Point::new(panel_x, panel_y),
                );
            } else if panel_bottom_right_shown {
                let panel_bottom_right = self
                    .panels
                    .get_mut(
                        &data
                            .panels
                            .get(&PanelPosition::BottomRight)
                            .unwrap()
                            .active,
                    )
                    .unwrap();
                panel_bottom_right.layout(
                    ctx,
                    &BoxConstraints::tight(Size::new(panel_width, bottom_height)),
                    data,
                    env,
                );
                panel_bottom_right.set_origin(
                    ctx,
                    data,
                    env,
                    Point::new(panel_x, panel_y),
                );
            }
            bottom_height
        } else {
            0.0
        };

        let main_split_size = Size::new(
            self_size.width - panel_left_width,
            self_size.height - status_size.height - panel_bottom_height,
        );
        let main_split_bc = BoxConstraints::tight(main_split_size);
        self.main_split.layout(ctx, &main_split_bc, data, env);
        self.main_split.set_origin(
            ctx,
            data,
            env,
            Point::new(panel_left_width, 0.0),
        );
        self.main_split_height = main_split_size.height;

        let completion_origin =
            data.completion_origin(ctx.text(), self_size.clone(), &data.config);
        self.completion.layout(ctx, bc, data, env);
        self.completion
            .set_origin(ctx, data, env, completion_origin);

        let code_action_origin =
            data.code_action_origin(ctx.text(), self_size.clone(), &data.config);
        self.code_action.layout(ctx, bc, data, env);
        self.code_action
            .set_origin(ctx, data, env, code_action_origin);

        let palette_size = self.palette.layout(ctx, bc, data, env);
        self.palette.set_origin(
            ctx,
            data,
            env,
            Point::new((self_size.width - palette_size.width) / 2.0, 0.0),
        );

        self_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &LapceTabData, env: &Env) {
        self.main_split.paint(ctx, data, env);
        for pos in &[
            PanelPosition::BottomLeft,
            PanelPosition::BottomRight,
            PanelPosition::LeftTop,
            PanelPosition::LeftBottom,
            PanelPosition::RightTop,
            PanelPosition::RightBottom,
        ] {
            if let Some(panel) = data.panels.get(&pos) {
                if panel.shown {
                    if let Some(panel) = self.panels.get_mut(&panel.active) {
                        let bg = match pos {
                            PanelPosition::LeftTop
                            | PanelPosition::LeftBottom
                            | PanelPosition::RightTop
                            | PanelPosition::RightBottom => data
                                .config
                                .get_color_unchecked(LapceTheme::PANEL_BACKGROUND),
                            PanelPosition::BottomLeft
                            | PanelPosition::BottomRight => data
                                .config
                                .get_color_unchecked(LapceTheme::EDITOR_BACKGROUND),
                        };
                        let rect = panel.layout_rect();
                        ctx.blurred_rect(
                            rect,
                            5.0,
                            data.config.get_color_unchecked(
                                LapceTheme::LAPCE_DROPDOWN_SHADOW,
                            ),
                        );
                        ctx.fill(rect, bg);
                        panel.paint(ctx, data, env);
                    }
                }
            }
        }
        self.status.paint(ctx, data, env);
        self.completion.paint(ctx, data, env);
        self.code_action.paint(ctx, data, env);
        self.palette.paint(ctx, data, env);
    }
}

pub struct LapceTabHeader {
    pub drag_start: Option<(Point, Point)>,
    pub mouse_pos: Point,
    cross_rect: Rect,
}

impl LapceTabHeader {
    pub fn new() -> Self {
        Self {
            cross_rect: Rect::ZERO,
            drag_start: None,
            mouse_pos: Point::ZERO,
        }
    }

    pub fn origin(&self) -> Option<Point> {
        self.drag_start
            .map(|(drag, origin)| origin + (self.mouse_pos - drag))
    }
}

impl Widget<LapceTabData> for LapceTabHeader {
    fn event(
        &mut self,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut LapceTabData,
        env: &Env,
    ) {
        match event {
            Event::MouseMove(mouse_event) => {
                if ctx.is_active() {
                    if let Some(pos) = self.drag_start {
                        self.mouse_pos = ctx.to_window(mouse_event.pos);
                        ctx.request_layout();
                    }
                    return;
                }
                if self.cross_rect.contains(mouse_event.pos) {
                    ctx.set_cursor(&druid::Cursor::Pointer);
                } else {
                    ctx.set_cursor(&druid::Cursor::Arrow);
                }
            }
            Event::MouseDown(mouse_event) => {
                if self.cross_rect.contains(mouse_event.pos) {
                    ctx.submit_command(Command::new(
                        LAPCE_UI_COMMAND,
                        LapceUICommand::CloseTabId(data.id),
                        Target::Auto,
                    ));
                } else {
                    self.drag_start =
                        Some((ctx.to_window(mouse_event.pos), ctx.window_origin()));
                    self.mouse_pos = ctx.to_window(mouse_event.pos);
                    ctx.set_active(true);
                    ctx.submit_command(Command::new(
                        LAPCE_UI_COMMAND,
                        LapceUICommand::FocusTabId(data.id),
                        Target::Auto,
                    ));
                }
            }
            Event::MouseUp(mouse_event) => {
                ctx.set_active(false);
                self.drag_start = None;
            }
            _ => {}
        }
    }

    fn lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        data: &LapceTabData,
        env: &Env,
    ) {
        match event {
            LifeCycle::HotChanged(is_hot) => {
                ctx.request_paint();
            }
            _ => (),
        }
    }

    fn update(
        &mut self,
        ctx: &mut druid::UpdateCtx,
        old_data: &LapceTabData,
        data: &LapceTabData,
        env: &Env,
    ) {
    }

    fn layout(
        &mut self,
        ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        data: &LapceTabData,
        env: &Env,
    ) -> Size {
        let size = bc.max();

        let cross_size = 8.0;
        let padding = (size.height - cross_size) / 2.0;
        let origin = Point::new(size.width - padding - cross_size, padding);
        self.cross_rect = Size::new(cross_size, cross_size)
            .to_rect()
            .with_origin(origin);

        size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &LapceTabData, env: &Env) {
        let dir = data
            .workspace
            .as_ref()
            .map(|w| {
                let dir = w.path.file_name().unwrap().to_str().unwrap();
                let dir = match &w.kind {
                    LapceWorkspaceType::Local => dir.to_string(),
                    LapceWorkspaceType::RemoteSSH(user, host) => {
                        format!("{} [{}@{}]", dir, user, host)
                    }
                };
                dir
            })
            .unwrap_or("Lapce".to_string());
        let text_layout = ctx
            .text()
            .new_text_layout(dir)
            .font(FontFamily::SYSTEM_UI, 13.0)
            .text_color(
                data.config
                    .get_color_unchecked(LapceTheme::EDITOR_FOREGROUND)
                    .clone(),
            )
            .build()
            .unwrap();

        let size = ctx.size();
        let text_size = text_layout.size();
        let x = (size.width - text_size.width) / 2.0;
        let y = (size.height - text_size.height) / 2.0;
        ctx.draw_text(&text_layout, Point::new(x, y));

        if ctx.is_hot() {
            let line = Line::new(
                Point::new(self.cross_rect.x0, self.cross_rect.y0),
                Point::new(self.cross_rect.x1, self.cross_rect.y1),
            );
            ctx.stroke(
                line,
                &data
                    .config
                    .get_color_unchecked(LapceTheme::EDITOR_FOREGROUND)
                    .clone(),
                1.0,
            );
            let line = Line::new(
                Point::new(self.cross_rect.x1, self.cross_rect.y0),
                Point::new(self.cross_rect.x0, self.cross_rect.y1),
            );
            ctx.stroke(
                line,
                &data
                    .config
                    .get_color_unchecked(LapceTheme::EDITOR_FOREGROUND)
                    .clone(),
                1.0,
            );
        }
    }
}