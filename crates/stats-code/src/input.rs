use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{self, IsTerminal, Write};

use crossterm::cursor::{MoveToColumn, MoveUp};
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode as CrosstermKeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, CompletionType, Config, Context, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers,
};

const MAX_VISIBLE_SLASH_COMMANDS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    Submit(String),
    Cancel,
    Exit,
}

struct SlashCommandHelper {
    completions: Vec<String>,
    current_line: RefCell<String>,
}

impl SlashCommandHelper {
    fn new(completions: Vec<String>) -> Self {
        Self {
            completions,
            current_line: RefCell::new(String::new()),
        }
    }

    fn reset_current_line(&self) {
        self.current_line.borrow_mut().clear();
    }

    fn current_line(&self) -> String {
        self.current_line.borrow().clone()
    }

    fn set_current_line(&self, line: &str) {
        let mut current = self.current_line.borrow_mut();
        current.clear();
        current.push_str(line);
    }
}

impl Completer for SlashCommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let Some(prefix) = slash_command_prefix(line, pos) else {
            return Ok((0, Vec::new()));
        };

        let matches = self
            .completions
            .iter()
            .filter(|candidate| candidate.starts_with(prefix))
            .map(|candidate| Pair {
                display: candidate.clone(),
                replacement: candidate.clone(),
            })
            .collect();

        Ok((0, matches))
    }
}

impl Hinter for SlashCommandHelper {
    type Hint = String;
}

impl Highlighter for SlashCommandHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        self.set_current_line(line);
        Cow::Borrowed(line)
    }

    fn highlight_char(&self, line: &str, _pos: usize, _kind: CmdKind) -> bool {
        self.set_current_line(line);
        false
    }
}

impl Validator for SlashCommandHelper {}
impl Helper for SlashCommandHelper {}

pub struct LineEditor {
    prompt: String,
    editor: Editor<SlashCommandHelper, DefaultHistory>,
}

enum InitialKeyRead {
    Slash,
    Prefill(String),
    UseRustyline,
    SubmitEmpty,
    Exit,
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

impl LineEditor {
    #[must_use]
    pub fn new(prompt: impl Into<String>, completions: Vec<String>) -> Self {
        let config = Config::builder()
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .build();
        let mut editor = Editor::<SlashCommandHelper, DefaultHistory>::with_config(config)
            .expect("rustyline editor should initialize");
        editor.set_helper(Some(SlashCommandHelper::new(completions)));
        editor.bind_sequence(KeyEvent(KeyCode::Char('J'), Modifiers::CTRL), Cmd::Newline);
        editor.bind_sequence(KeyEvent(KeyCode::Enter, Modifiers::SHIFT), Cmd::Newline);

        Self {
            prompt: prompt.into(),
            editor,
        }
    }

    pub fn push_history(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        if entry.trim().is_empty() {
            return;
        }

        let _ = self.editor.add_history_entry(entry);
    }

    pub fn read_line(&mut self) -> io::Result<ReadOutcome> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return self.read_line_fallback();
        }

        if let Some(helper) = self.editor.helper_mut() {
            helper.reset_current_line();
        }

        match self.read_initial_key()? {
            InitialKeyRead::Slash => self.read_slash_line(),
            InitialKeyRead::Prefill(initial) => {
                Self::clear_prompt_line()?;
                self.read_with_rustyline(Some(initial))
            }
            InitialKeyRead::UseRustyline => {
                Self::clear_prompt_line()?;
                self.read_with_rustyline(None)
            }
            InitialKeyRead::SubmitEmpty => {
                let mut stdout = io::stdout();
                writeln!(stdout)?;
                Ok(ReadOutcome::Submit(String::new()))
            }
            InitialKeyRead::Exit => {
                let mut stdout = io::stdout();
                writeln!(stdout)?;
                Ok(ReadOutcome::Exit)
            }
        }
    }

    fn read_with_rustyline(&mut self, initial: Option<String>) -> io::Result<ReadOutcome> {
        let readline = match initial {
            Some(initial) => self
                .editor
                .readline_with_initial(&self.prompt, (&initial, "")),
            None => self.editor.readline(&self.prompt),
        };

        match readline {
            Ok(line) => Ok(ReadOutcome::Submit(line)),
            Err(ReadlineError::Interrupted) => {
                let has_input = !self.current_line().is_empty();
                self.finish_interrupted_read()?;
                if has_input {
                    Ok(ReadOutcome::Cancel)
                } else {
                    Ok(ReadOutcome::Exit)
                }
            }
            Err(ReadlineError::Eof) => {
                self.finish_interrupted_read()?;
                Ok(ReadOutcome::Exit)
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn read_initial_key(&self) -> io::Result<InitialKeyRead> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", self.prompt)?;
        stdout.flush()?;

        let _raw_mode = RawModeGuard::new()?;
        loop {
            let CrosstermEvent::Key(key) = event::read()? else {
                continue;
            };

            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }

            return Ok(match (key.code, key.modifiers) {
                (CrosstermKeyCode::Char('/'), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    InitialKeyRead::Slash
                }
                (CrosstermKeyCode::Char('c'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    InitialKeyRead::Exit
                }
                (CrosstermKeyCode::Char('d'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    InitialKeyRead::Exit
                }
                (CrosstermKeyCode::Enter, _) => InitialKeyRead::SubmitEmpty,
                (CrosstermKeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    InitialKeyRead::Prefill(ch.to_string())
                }
                _ => InitialKeyRead::UseRustyline,
            });
        }
    }

    fn read_slash_line(&mut self) -> io::Result<ReadOutcome> {
        let mut stdout = io::stdout();
        let _raw_mode = RawModeGuard::new()?;
        let completions = self.slash_completions();
        let mut buffer = "/".to_string();
        let mut selected = 0usize;

        loop {
            let matches = filter_slash_completions(&completions, &buffer);
            if matches.is_empty() {
                selected = 0;
            } else {
                selected = selected.min(matches.len() - 1);
            }
            self.render_slash_menu(&mut stdout, &buffer, &matches, selected)?;

            let CrosstermEvent::Key(key) = event::read()? else {
                continue;
            };
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }

            match (key.code, key.modifiers) {
                (CrosstermKeyCode::Char('c'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    self.finish_slash_menu(&mut stdout, &buffer)?;
                    return Ok(ReadOutcome::Cancel);
                }
                (CrosstermKeyCode::Char('d'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    if buffer.is_empty() {
                        self.finish_slash_menu(&mut stdout, &buffer)?;
                        return Ok(ReadOutcome::Exit);
                    }
                }
                (CrosstermKeyCode::Enter, _) => {
                    self.finish_slash_menu(&mut stdout, &buffer)?;
                    return Ok(ReadOutcome::Submit(buffer));
                }
                (CrosstermKeyCode::Esc, _) => {
                    self.finish_slash_menu(&mut stdout, &buffer)?;
                    return Ok(ReadOutcome::Cancel);
                }
                (CrosstermKeyCode::Backspace, _) => {
                    buffer.pop();
                }
                (CrosstermKeyCode::Up, _) => {
                    if !matches.is_empty() {
                        selected = selected.checked_sub(1).unwrap_or(matches.len() - 1);
                    }
                }
                (CrosstermKeyCode::Down, _) => {
                    if !matches.is_empty() {
                        selected = (selected + 1) % matches.len();
                    }
                }
                (CrosstermKeyCode::Tab, _) => {
                    if let Some(choice) = matches.get(selected) {
                        buffer.clone_from(choice);
                    }
                }
                (CrosstermKeyCode::Char(ch), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    buffer.push(ch);
                }
                _ => {}
            }
        }
    }

    fn current_line(&self) -> String {
        self.editor
            .helper()
            .map_or_else(String::new, SlashCommandHelper::current_line)
    }

    fn slash_completions(&self) -> Vec<String> {
        self.editor
            .helper()
            .map(|helper| helper.completions.clone())
            .unwrap_or_default()
    }

    fn clear_prompt_line() -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))
    }

    fn render_slash_menu(
        &self,
        stdout: &mut io::Stdout,
        buffer: &str,
        matches: &[String],
        selected: usize,
    ) -> io::Result<()> {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
        write!(stdout, "{}{}", self.prompt, buffer)?;

        let visible_count = matches.len().min(MAX_VISIBLE_SLASH_COMMANDS);
        for (index, candidate) in matches.iter().take(MAX_VISIBLE_SLASH_COMMANDS).enumerate() {
            write!(
                stdout,
                "\r\n{} {}",
                if index == selected { ">" } else { " " },
                candidate
            )?;
        }
        if matches.len() > MAX_VISIBLE_SLASH_COMMANDS {
            write!(
                stdout,
                "\r\n  ... {} more",
                matches.len() - MAX_VISIBLE_SLASH_COMMANDS
            )?;
        }

        let menu_lines = visible_count + usize::from(matches.len() > MAX_VISIBLE_SLASH_COMMANDS);
        if menu_lines > 0 {
            execute!(
                stdout,
                MoveUp(menu_lines as u16),
                MoveToColumn(prompt_column(&self.prompt, buffer))
            )?;
        }
        stdout.flush()
    }

    fn finish_slash_menu(&self, stdout: &mut io::Stdout, buffer: &str) -> io::Result<()> {
        execute!(stdout, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
        write!(stdout, "{}{}", self.prompt, buffer)?;
        writeln!(stdout)
    }

    fn finish_interrupted_read(&mut self) -> io::Result<()> {
        if let Some(helper) = self.editor.helper_mut() {
            helper.reset_current_line();
        }
        let mut stdout = io::stdout();
        writeln!(stdout)
    }

    fn read_line_fallback(&self) -> io::Result<ReadOutcome> {
        let mut stdout = io::stdout();
        write!(stdout, "{}", self.prompt)?;
        stdout.flush()?;

        let mut buffer = String::new();
        let bytes_read = io::stdin().read_line(&mut buffer)?;
        if bytes_read == 0 {
            return Ok(ReadOutcome::Exit);
        }

        while matches!(buffer.chars().last(), Some('\n' | '\r')) {
            buffer.pop();
        }
        Ok(ReadOutcome::Submit(buffer))
    }
}

fn slash_command_prefix(line: &str, pos: usize) -> Option<&str> {
    if pos != line.len() {
        return None;
    }

    let prefix = &line[..pos];
    if prefix.contains(char::is_whitespace) || !prefix.starts_with('/') {
        return None;
    }

    Some(prefix)
}

fn filter_slash_completions(completions: &[String], line: &str) -> Vec<String> {
    let Some(prefix) = slash_command_prefix(line, line.len()) else {
        return Vec::new();
    };

    completions
        .iter()
        .filter(|candidate| candidate.starts_with(prefix))
        .cloned()
        .collect()
}

fn prompt_column(prompt: &str, buffer: &str) -> u16 {
    let width = prompt.chars().count() + buffer.chars().count();
    width.min(u16::MAX as usize) as u16
}

#[cfg(test)]
mod tests {
    use super::{
        filter_slash_completions, prompt_column, slash_command_prefix, LineEditor,
        SlashCommandHelper,
    };
    use rustyline::completion::Completer;
    use rustyline::highlight::Highlighter;
    use rustyline::history::{DefaultHistory, History};
    use rustyline::Context;

    #[test]
    fn extracts_only_terminal_slash_command_prefixes() {
        assert_eq!(slash_command_prefix("/he", 3), Some("/he"));
        assert_eq!(slash_command_prefix("/help me", 5), None);
        assert_eq!(slash_command_prefix("hello", 5), None);
        assert_eq!(slash_command_prefix("/help", 2), None);
    }

    #[test]
    fn completes_matching_slash_commands() {
        let helper = SlashCommandHelper::new(vec![
            "/help".to_string(),
            "/hello".to_string(),
            "/status".to_string(),
        ]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, matches) = helper
            .complete("/he", 3, &ctx)
            .expect("completion should work");

        assert_eq!(start, 0);
        assert_eq!(
            matches
                .into_iter()
                .map(|candidate| candidate.replacement)
                .collect::<Vec<_>>(),
            vec!["/help".to_string(), "/hello".to_string()]
        );
    }

    #[test]
    fn ignores_non_slash_command_completion_requests() {
        let helper = SlashCommandHelper::new(vec!["/help".to_string()]);
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (_, matches) = helper
            .complete("hello", 5, &ctx)
            .expect("completion should work");

        assert!(matches.is_empty());
    }

    #[test]
    fn tracks_current_buffer_through_highlighter() {
        let helper = SlashCommandHelper::new(Vec::new());
        let _ = helper.highlight("draft", 5);

        assert_eq!(helper.current_line(), "draft");
    }

    #[test]
    fn push_history_ignores_blank_entries() {
        let mut editor = LineEditor::new("> ", vec!["/help".to_string()]);
        editor.push_history("   ");
        editor.push_history("/help");

        assert_eq!(editor.editor.history().len(), 1);
    }

    #[test]
    fn filters_slash_completions_by_prefix() {
        let completions = vec![
            "/help".to_string(),
            "/hello".to_string(),
            "/status".to_string(),
        ];

        assert_eq!(
            filter_slash_completions(&completions, "/he"),
            vec!["/help".to_string(), "/hello".to_string()]
        );
        assert!(filter_slash_completions(&completions, "/help me").is_empty());
    }

    #[test]
    fn prompt_column_tracks_prompt_and_buffer_width() {
        assert_eq!(prompt_column("> ", "/help"), 7);
    }
}
