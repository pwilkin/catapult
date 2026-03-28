use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Widget},
};
use tui_input::Input;

pub struct AutocompleteState {
    pub input: Input,
    pub items: Vec<AutocompleteItem>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub open: bool,
    pub max_visible: usize,
}

#[derive(Clone)]
pub struct AutocompleteItem {
    pub key: String,
    pub label: String,
    pub description: String,
}

impl AutocompleteState {
    pub fn new(items: Vec<AutocompleteItem>) -> Self {
        let all_indices: Vec<usize> = (0..items.len()).collect();
        Self {
            input: Input::default(),
            items,
            filtered: all_indices,
            selected: 0,
            open: false,
            max_visible: 8,
        }
    }

    pub fn update_filter(&mut self) {
        let query = self.input.value().to_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            let mut scored: Vec<(usize, i32)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    let key_lower = item.key.to_lowercase();
                    let label_lower = item.label.to_lowercase();
                    let desc_lower = item.description.to_lowercase();

                    if key_lower.starts_with(&query) {
                        Some((i, 100))
                    } else if key_lower.contains(&query) {
                        Some((i, 80))
                    } else if label_lower.contains(&query) {
                        Some((i, 60))
                    } else if desc_lower.contains(&query) {
                        Some((i, 40))
                    } else {
                        // Fuzzy: check if all query chars appear in order
                        let mut qi = 0;
                        let query_chars: Vec<char> = query.chars().collect();
                        for c in key_lower.chars().chain(desc_lower.chars()) {
                            if qi < query_chars.len() && c == query_chars[qi] {
                                qi += 1;
                            }
                        }
                        if qi == query_chars.len() {
                            Some((i, 20))
                        } else {
                            None
                        }
                    }
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        self.selected = 0;
        self.open = !self.filtered.is_empty();
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.selected < self.filtered.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn selected_item(&self) -> Option<&AutocompleteItem> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.items.get(i))
    }

    pub fn reset(&mut self) {
        self.input = Input::default();
        self.filtered = (0..self.items.len()).collect();
        self.selected = 0;
        self.open = false;
    }
}

#[allow(dead_code)]
pub fn render_autocomplete_input(
    state: &AutocompleteState,
    area: Rect,
    buf: &mut Buffer,
    focused: bool,
    label: &str,
) {
    let input_style = if focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Blue)
    };

    let value = state.input.value();
    let prefix = format!("{}: ", label);

    let line = Line::from(vec![
        Span::styled(&prefix, Style::default().fg(Color::Cyan)),
        Span::styled(value, input_style),
        if focused && value.is_empty() {
            Span::styled("type to search...", Style::default().fg(Color::Blue))
        } else {
            Span::raw("")
        },
    ]);
    line.render(area, buf);

    // Cursor position for the terminal (handled by the caller via set_cursor)
}

pub fn render_autocomplete_dropdown(
    state: &AutocompleteState,
    area: Rect,
    buf: &mut Buffer,
) {
    if !state.open || state.filtered.is_empty() {
        return;
    }

    let visible_count = state.filtered.len().min(state.max_visible);
    let scroll_offset = if state.selected >= visible_count {
        state.selected - visible_count + 1
    } else {
        0
    };

    let dropdown_height = visible_count as u16 + 2; // +2 for borders
    if area.height < dropdown_height {
        return;
    }

    let dropdown_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.min(60),
        height: dropdown_height,
    };

    // Clear the area first
    Clear.render(dropdown_area, buf);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let inner = block.inner(dropdown_area);
    block.render(dropdown_area, buf);

    for (vi, &fi) in state
        .filtered
        .iter()
        .skip(scroll_offset)
        .take(visible_count)
        .enumerate()
    {
        let item = &state.items[fi];
        let is_selected = scroll_offset + vi == state.selected;

        let style = if is_selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let y = inner.y + vi as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let row_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };

        // Format: "key          description"
        let key_width = 18.min(inner.width as usize / 3);
        let key_display = if item.key.len() > key_width {
            &item.key[..key_width]
        } else {
            &item.key
        };
        let padding = key_width.saturating_sub(key_display.len());
        let desc_width = (inner.width as usize).saturating_sub(key_width + 1);
        let desc = if item.description.len() > desc_width {
            &item.description[..desc_width]
        } else {
            &item.description
        };

        let line = Line::from(vec![
            Span::styled(
                format!("{}{} ", key_display, " ".repeat(padding)),
                style,
            ),
            Span::styled(
                desc.to_string(),
                if is_selected {
                    style
                } else {
                    Style::default().fg(Color::Blue)
                },
            ),
        ]);
        line.render(row_area, buf);
    }
}
