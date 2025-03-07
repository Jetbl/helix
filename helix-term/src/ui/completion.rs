use crate::compositor::{Component, Context, EventResult};
use crossterm::event::{Event, KeyCode, KeyEvent};
use tui::buffer::Buffer as Surface;

use std::borrow::Cow;

use helix_core::Transaction;
use helix_view::{graphics::Rect, Document, Editor};

use crate::commands;
use crate::ui::{menu, Markdown, Menu, Popup, PromptEvent};

use helix_lsp::{lsp, util};
use lsp::CompletionItem;

impl menu::Item for CompletionItem {
    fn sort_text(&self) -> &str {
        self.filter_text.as_ref().unwrap_or(&self.label).as_str()
    }

    fn filter_text(&self) -> &str {
        self.filter_text.as_ref().unwrap_or(&self.label).as_str()
    }

    fn label(&self) -> &str {
        self.label.as_str()
    }

    fn row(&self) -> menu::Row {
        menu::Row::new(vec![
            menu::Cell::from(self.label.as_str()),
            menu::Cell::from(match self.kind {
                Some(lsp::CompletionItemKind::TEXT) => "text",
                Some(lsp::CompletionItemKind::METHOD) => "method",
                Some(lsp::CompletionItemKind::FUNCTION) => "function",
                Some(lsp::CompletionItemKind::CONSTRUCTOR) => "constructor",
                Some(lsp::CompletionItemKind::FIELD) => "field",
                Some(lsp::CompletionItemKind::VARIABLE) => "variable",
                Some(lsp::CompletionItemKind::CLASS) => "class",
                Some(lsp::CompletionItemKind::INTERFACE) => "interface",
                Some(lsp::CompletionItemKind::MODULE) => "module",
                Some(lsp::CompletionItemKind::PROPERTY) => "property",
                Some(lsp::CompletionItemKind::UNIT) => "unit",
                Some(lsp::CompletionItemKind::VALUE) => "value",
                Some(lsp::CompletionItemKind::ENUM) => "enum",
                Some(lsp::CompletionItemKind::KEYWORD) => "keyword",
                Some(lsp::CompletionItemKind::SNIPPET) => "snippet",
                Some(lsp::CompletionItemKind::COLOR) => "color",
                Some(lsp::CompletionItemKind::FILE) => "file",
                Some(lsp::CompletionItemKind::REFERENCE) => "reference",
                Some(lsp::CompletionItemKind::FOLDER) => "folder",
                Some(lsp::CompletionItemKind::ENUM_MEMBER) => "enum_member",
                Some(lsp::CompletionItemKind::CONSTANT) => "constant",
                Some(lsp::CompletionItemKind::STRUCT) => "struct",
                Some(lsp::CompletionItemKind::EVENT) => "event",
                Some(lsp::CompletionItemKind::OPERATOR) => "operator",
                Some(lsp::CompletionItemKind::TYPE_PARAMETER) => "type_param",
                Some(kind) => unimplemented!("{:?}", kind),
                None => "",
            }),
            // self.detail.as_deref().unwrap_or("")
            // self.label_details
            //     .as_ref()
            //     .or(self.detail())
            //     .as_str(),
        ])
    }
}

/// Wraps a Menu.
pub struct Completion {
    popup: Popup<Menu<CompletionItem>>,
    start_offset: usize,
    #[allow(dead_code)]
    trigger_offset: usize,
    // TODO: maintain a completioncontext with trigger kind & trigger char
}

impl Completion {
    pub fn new(
        editor: &Editor,
        items: Vec<CompletionItem>,
        offset_encoding: helix_lsp::OffsetEncoding,
        start_offset: usize,
        trigger_offset: usize,
    ) -> Self {
        let menu = Menu::new(items, move |editor: &mut Editor, item, event| {
            fn item_to_transaction(
                doc: &Document,
                item: &CompletionItem,
                offset_encoding: helix_lsp::OffsetEncoding,
                start_offset: usize,
                trigger_offset: usize,
            ) -> Transaction {
                if let Some(edit) = &item.text_edit {
                    let edit = match edit {
                        lsp::CompletionTextEdit::Edit(edit) => edit.clone(),
                        lsp::CompletionTextEdit::InsertAndReplace(item) => {
                            unimplemented!("completion: insert_and_replace {:?}", item)
                        }
                    };
                    util::generate_transaction_from_edits(
                        doc.text(),
                        vec![edit],
                        offset_encoding, // TODO: should probably transcode in Client
                    )
                } else {
                    let text = item.insert_text.as_ref().unwrap_or(&item.label);
                    // Some LSPs just give you an insertText with no offset ¯\_(ツ)_/¯
                    // in these cases we need to check for a common prefix and remove it
                    let prefix = Cow::from(doc.text().slice(start_offset..trigger_offset));
                    let text = text.trim_start_matches::<&str>(&prefix);
                    Transaction::change(
                        doc.text(),
                        vec![(trigger_offset, trigger_offset, Some(text.into()))].into_iter(),
                    )
                }
            }

            let (view, doc) = current!(editor);

            // if more text was entered, remove it
            doc.restore(view.id);

            match event {
                PromptEvent::Abort => {}
                PromptEvent::Update => {
                    // always present here
                    let item = item.unwrap();

                    let transaction = item_to_transaction(
                        doc,
                        item,
                        offset_encoding,
                        start_offset,
                        trigger_offset,
                    );

                    // initialize a savepoint
                    doc.savepoint();

                    doc.apply(&transaction, view.id);
                }
                PromptEvent::Validate => {
                    // always present here
                    let item = item.unwrap();

                    let transaction = item_to_transaction(
                        doc,
                        item,
                        offset_encoding,
                        start_offset,
                        trigger_offset,
                    );
                    doc.apply(&transaction, view.id);

                    if let Some(additional_edits) = &item.additional_text_edits {
                        // gopls uses this to add extra imports
                        if !additional_edits.is_empty() {
                            let transaction = util::generate_transaction_from_edits(
                                doc.text(),
                                additional_edits.clone(),
                                offset_encoding, // TODO: should probably transcode in Client
                            );
                            doc.apply(&transaction, view.id);
                        }
                    }
                }
            };
        });
        let popup = Popup::new(menu);
        let mut completion = Self {
            popup,
            start_offset,
            trigger_offset,
        };

        // need to recompute immediately in case start_offset != trigger_offset
        completion.recompute_filter(editor);

        completion
    }

    pub fn recompute_filter(&mut self, editor: &Editor) {
        // recompute menu based on matches
        let menu = self.popup.contents_mut();
        let (view, doc) = current_ref!(editor);

        // cx.hooks()
        // cx.add_hook(enum type,  ||)
        // cx.trigger_hook(enum type, &str, ...) <-- there has to be enough to identify doc/view
        // callback with editor & compositor
        //
        // trigger_hook sends event into channel, that's consumed in the global loop and
        // triggers all registered callbacks
        // TODO: hooks should get processed immediately so maybe do it after select!(), before
        // looping?

        let cursor = doc
            .selection(view.id)
            .primary()
            .cursor(doc.text().slice(..));
        if self.trigger_offset <= cursor {
            let fragment = doc.text().slice(self.start_offset..cursor);
            let text = Cow::from(fragment);
            // TODO: logic is same as ui/picker
            menu.score(&text);
        } else {
            // we backspaced before the start offset, clear the menu
            // this will cause the editor to remove the completion popup
            menu.clear();
        }
    }

    pub fn update(&mut self, cx: &mut commands::Context) {
        self.recompute_filter(cx.editor)
    }

    pub fn is_empty(&self) -> bool {
        self.popup.contents().is_empty()
    }
}

// need to:
// - trigger on the right trigger char
//   - detect previous open instance and recycle
// - update after input, but AFTER the document has changed
// - if no more matches, need to auto close
//
// missing bits:
// - a more robust hook system: emit to a channel, process in main loop
// - a way to find specific layers in compositor
// - components register for hooks, then unregister when terminated
// ... since completion is a special case, maybe just build it into doc/render?

impl Component for Completion {
    fn handle_event(&mut self, event: Event, cx: &mut Context) -> EventResult {
        // let the Editor handle Esc instead
        if let Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) = event
        {
            return EventResult::Ignored;
        }
        self.popup.handle_event(event, cx)
    }

    fn required_size(&mut self, viewport: (u16, u16)) -> Option<(u16, u16)> {
        self.popup.required_size(viewport)
    }

    fn render(&mut self, area: Rect, surface: &mut Surface, cx: &mut Context) {
        self.popup.render(area, surface, cx);

        // if we have a selection, render a markdown popup on top/below with info
        if let Some(option) = self.popup.contents().selection() {
            // need to render:
            // option.detail
            // ---
            // option.documentation

            let (view, doc) = current!(cx.editor);
            let language = doc
                .language()
                .and_then(|scope| scope.strip_prefix("source."))
                .unwrap_or("");
            let text = doc.text().slice(..);
            let cursor_pos = doc.selection(view.id).primary().cursor(text);
            let coords = helix_core::visual_coords_at_pos(text, cursor_pos, doc.tab_width());
            let cursor_pos = (coords.row - view.offset.row) as u16;
            let mut markdown_doc = match &option.documentation {
                Some(lsp::Documentation::String(contents))
                | Some(lsp::Documentation::MarkupContent(lsp::MarkupContent {
                    kind: lsp::MarkupKind::PlainText,
                    value: contents,
                })) => {
                    // TODO: convert to wrapped text
                    Markdown::new(
                        format!(
                            "```{}\n{}\n```\n{}",
                            language,
                            option.detail.as_deref().unwrap_or_default(),
                            contents.clone()
                        ),
                        cx.editor.syn_loader.clone(),
                    )
                }
                Some(lsp::Documentation::MarkupContent(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: contents,
                })) => {
                    // TODO: set language based on doc scope
                    Markdown::new(
                        format!(
                            "```{}\n{}\n```\n{}",
                            language,
                            option.detail.as_deref().unwrap_or_default(),
                            contents.clone()
                        ),
                        cx.editor.syn_loader.clone(),
                    )
                }
                None if option.detail.is_some() => {
                    // TODO: copied from above

                    // TODO: set language based on doc scope
                    Markdown::new(
                        format!(
                            "```{}\n{}\n```",
                            language,
                            option.detail.as_deref().unwrap_or_default(),
                        ),
                        cx.editor.syn_loader.clone(),
                    )
                }
                None => return,
            };

            let (popup_x, popup_y) = self.popup.get_rel_position(area, cx);
            let (popup_width, _popup_height) = self.popup.get_size();
            let mut width = area
                .width
                .saturating_sub(popup_x)
                .saturating_sub(popup_width);
            let area = if width > 30 {
                let mut height = area.height.saturating_sub(popup_y);
                let x = popup_x + popup_width;
                let y = popup_y;

                if let Some((rel_width, rel_height)) = markdown_doc.required_size((width, height)) {
                    width = rel_width;
                    height = rel_height;
                }
                Rect::new(x, y, width, height)
            } else {
                let half = area.height / 2;
                let height = 15.min(half);
                // we want to make sure the cursor is visible (not hidden behind the documentation)
                let y = if cursor_pos + area.y
                    >= (cx.editor.tree.area().height - height - 2/* statusline + commandline */)
                {
                    0
                } else {
                    // -2 to subtract command line + statusline. a bit of a hack, because of splits.
                    area.height.saturating_sub(height).saturating_sub(2)
                };

                Rect::new(0, y, area.width, height)
            };

            // clear area
            let background = cx.editor.theme.get("ui.popup");
            surface.clear_with(area, background);
            markdown_doc.render(area, surface, cx);
        }
    }
}
