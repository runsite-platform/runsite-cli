//! Every key the TUI understands, one table per keyboard mode. Key handling,
//! the status bar hints and the help overlay all read these tables, so a key
//! cannot work without being documented or be documented without working.

use super::actions::ActionKey;
use super::app::{App, Overlay, Screen};
use super::dashboard::Pane;
use super::detail::DetailTab;
use super::login::LoginTab;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the keyboard currently drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    LoginForm,
    /// Left the login form with Esc: global keys work.
    LoginNavigating,
    Blocked,
    Dashboard(Pane),
    DashboardFilter,
    ServiceOverview,
    ServiceLogs {
        wrap: bool,
    },
    ServiceDeploys,
    DeploymentView,
    LogSearch,
    DatabaseDetail,
    Help,
    ProfilePicker,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// This key without Ctrl; Shift is allowed, as uppercase letters carry it.
    Plain(KeyCode),
    Ctrl(KeyCode),
    /// Any character without Ctrl, for text fields.
    AnyChar,
    Anything,
}

impl Key {
    pub fn matches(self, event: &KeyEvent) -> bool {
        let control = event.modifiers.contains(KeyModifiers::CONTROL);
        match self {
            Key::Plain(code) => event.code == code && !control,
            Key::Ctrl(code) => event.code == code && control,
            Key::AnyChar => matches!(event.code, KeyCode::Char(_)) && !control,
            Key::Anything => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    OpenHelp,
    Refresh,
    OpenProfiles,
    Action(ActionKey),
    CloseOverlay,
    PickerUp,
    PickerDown,
    PickerChoose,
    ConfirmAction,
    CancelAction,
    ReturnToLoginForm,
    LoginTab(LoginTab),
    LoginFocus {
        forward: bool,
    },
    LoginEscape,
    LoginErase,
    LoginSubmit,
    LoginType,
    FilterEscape,
    FilterApply,
    FilterErase,
    FilterType,
    SwitchPane,
    CursorUp,
    CursorDown,
    FocusResources,
    FocusProjects,
    OpenResource,
    /// Opens the selected service on the tab named by the pressed digit.
    OpenResourceOnTab,
    OpenFilter,
    BackToDashboard,
    Detail(DetailCommand),
}

/// Commands on the service detail screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailCommand {
    /// The tab named by the pressed digit.
    TabByDigit,
    NextTab,
    PreviousTab,
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Top,
    Bottom,
    ToggleFollow,
    ToggleWrap,
    ClearView,
    OpenSearch,
    NextMatch,
    PreviousMatch,
    ScrollLeft,
    ScrollRight,
    SearchEscape,
    SearchApply,
    SearchErase,
    SearchType,
    DeployCursorUp,
    DeployCursorDown,
    OpenDeployment,
    CloseDeployment,
}

/// A status bar entry: the key as shown and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hint {
    pub key: &'static str,
    pub text: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub keys: &'static [Key],
    pub command: Command,
    /// The keys as the help overlay names them; `None` keeps the binding out
    /// of the help, for a key already covered by a neighbour's line.
    pub help: Option<(&'static str, &'static str)>,
    pub hint: Option<Hint>,
}

const fn bind(keys: &'static [Key], command: Command) -> Binding {
    Binding {
        keys,
        command,
        help: None,
        hint: None,
    }
}

impl Binding {
    const fn help(mut self, keys: &'static str, text: &'static str) -> Self {
        self.help = Some((keys, text));
        self
    }

    const fn hint(mut self, key: &'static str, text: &'static str) -> Self {
        self.hint = Some(Hint { key, text });
        self
    }

    /// The same key, kept out of the status bar to save room.
    const fn unhinted(mut self) -> Self {
        self.hint = None;
        self
    }
}

const fn plain(character: char) -> Key {
    Key::Plain(KeyCode::Char(character))
}

const fn action(key: ActionKey, keys: &'static [Key]) -> Binding {
    Binding {
        keys,
        command: Command::Action(key),
        help: Some((key.key(), key.help())),
        hint: None,
    }
}

const DEPLOY: Binding = action(ActionKey::Deploy, &[plain('D')]);
const RESTART: Binding = action(ActionKey::Restart, &[plain('R')]);
const START_STOP: Binding = action(ActionKey::StartStop, &[plain('S')]);
const ROLLBACK: Binding = action(ActionKey::Rollback, &[plain('B')]);

const UP: &[Key] = &[Key::Plain(KeyCode::Up), plain('k')];
const DOWN: &[Key] = &[Key::Plain(KeyCode::Down), plain('j')];
const ESC: &[Key] = &[Key::Plain(KeyCode::Esc)];
const ENTER: &[Key] = &[Key::Plain(KeyCode::Enter)];
const BACKSPACE: &[Key] = &[Key::Plain(KeyCode::Backspace)];
const DIGITS: &[Key] = &[plain('1'), plain('2'), plain('3')];

/// Shared by every navigation screen.
const GLOBAL: [Binding; 4] = [
    bind(&[plain('r')], Command::Refresh).help("r", "refresh now"),
    bind(&[plain('P')], Command::OpenProfiles).help("P", "switch profile"),
    bind(&[plain('?')], Command::OpenHelp)
        .help("?", "this help")
        .hint("?", "help"),
    bind(&[plain('q')], Command::Quit)
        .help("q / ctrl+c", "quit")
        .hint("q", "quit"),
];

/// Detail screens have more to show, so quitting is left to the help.
const DETAIL_GLOBAL: [Binding; 4] = [GLOBAL[0], GLOBAL[1], GLOBAL[2], GLOBAL[3].unhinted()];

const LOGIN_FORM: &[Binding] = &[
    bind(
        &[Key::Ctrl(KeyCode::Left)],
        Command::LoginTab(LoginTab::Password),
    )
    .help("ctrl+← / ctrl+→", "switch tab")
    .hint("ctrl+←/→", "tab"),
    bind(
        &[Key::Ctrl(KeyCode::Right)],
        Command::LoginTab(LoginTab::ApiKey),
    ),
    bind(
        &[Key::Plain(KeyCode::Tab)],
        Command::LoginFocus { forward: true },
    )
    .help("tab / shift+tab", "next / previous field")
    .hint("tab", "next field"),
    bind(
        &[Key::Plain(KeyCode::BackTab)],
        Command::LoginFocus { forward: false },
    ),
    bind(ENTER, Command::LoginSubmit)
        .help("enter", "log in")
        .hint("enter", "log in"),
    bind(ESC, Command::LoginEscape).help("esc", "clear the field; on an empty one leave the form"),
    bind(BACKSPACE, Command::LoginErase),
    bind(&[Key::AnyChar], Command::LoginType),
    bind(&[Key::Ctrl(KeyCode::Char('c'))], Command::Quit).hint("ctrl+c", "quit"),
];

const LOGIN_NAVIGATING: &[Binding] = &[
    bind(
        &[
            Key::Plain(KeyCode::Enter),
            Key::Plain(KeyCode::Tab),
            plain('i'),
        ],
        Command::ReturnToLoginForm,
    )
    .help("enter / tab / i", "back to the form")
    .hint("enter", "back to the form"),
    GLOBAL[0],
    GLOBAL[1].hint("P", "profile"),
    GLOBAL[2],
    GLOBAL[3],
];

const BLOCKED: &[Binding] = &[bind(&[plain('q')], Command::Quit)
    .help("q", "quit")
    .hint("q", "quit")];

const DASHBOARD_TOP: [Binding; 3] = [
    bind(UP, Command::CursorUp).help("↑↓ / j k", "move"),
    bind(DOWN, Command::CursorDown),
    bind(
        &[
            Key::Plain(KeyCode::Tab),
            Key::Plain(KeyCode::Left),
            Key::Plain(KeyCode::Right),
            plain('h'),
            plain('l'),
        ],
        Command::SwitchPane,
    )
    .help("tab / ←→ / h l", "switch pane"),
];

const OPEN_FILTER: Binding = bind(&[plain('/')], Command::OpenFilter)
    .help("/", "filter the focused list")
    .hint("/", "filter");

const DASHBOARD_PROJECTS: &[Binding] = &[
    bind(ENTER, Command::FocusResources)
        .help("enter", "select a project, open a service")
        .hint("enter", "select"),
    DASHBOARD_TOP[0],
    DASHBOARD_TOP[1],
    DASHBOARD_TOP[2],
    OPEN_FILTER,
    GLOBAL[0],
    GLOBAL[1],
    GLOBAL[2],
    GLOBAL[3],
];

const DASHBOARD_RESOURCES: &[Binding] = &[
    bind(ENTER, Command::OpenResource)
        .help("enter", "select a project, open a service")
        .hint("enter", "open"),
    DEPLOY,
    RESTART,
    START_STOP,
    DASHBOARD_TOP[0],
    DASHBOARD_TOP[1],
    DASHBOARD_TOP[2],
    bind(DIGITS, Command::OpenResourceOnTab).help("1 2 3", "open a service on a tab"),
    bind(ESC, Command::FocusProjects).help("esc", "back to projects"),
    OPEN_FILTER,
    GLOBAL[0],
    GLOBAL[1],
    GLOBAL[2],
    GLOBAL[3],
];

const DASHBOARD_FILTER: &[Binding] = &[
    bind(ENTER, Command::FilterApply).hint("enter", "apply"),
    bind(ESC, Command::FilterEscape).hint("esc", "clear / close"),
    bind(BACKSPACE, Command::FilterErase),
    bind(&[Key::AnyChar], Command::FilterType),
];

const fn detail(keys: &'static [Key], command: DetailCommand) -> Binding {
    bind(keys, Command::Detail(command))
}

const TAB_KEYS: [Binding; 3] = [
    detail(DIGITS, DetailCommand::TabByDigit)
        .help("1 2 3 / tab", "switch tab")
        .hint("1-3", "tabs"),
    detail(&[Key::Plain(KeyCode::Tab)], DetailCommand::NextTab),
    detail(&[Key::Plain(KeyCode::BackTab)], DetailCommand::PreviousTab),
];

const ARROW_TABS: [Binding; 2] = [
    detail(
        &[Key::Plain(KeyCode::Right), plain('l')],
        DetailCommand::NextTab,
    ),
    detail(
        &[Key::Plain(KeyCode::Left), plain('h')],
        DetailCommand::PreviousTab,
    ),
];

const BACK: Binding = bind(ESC, Command::BackToDashboard)
    .help("esc", "back")
    .hint("esc", "back");

const SERVICE_OVERVIEW: &[Binding] = &[
    TAB_KEYS[0],
    DEPLOY,
    RESTART,
    START_STOP,
    TAB_KEYS[1],
    TAB_KEYS[2],
    ARROW_TABS[0],
    ARROW_TABS[1],
    BACK,
    DETAIL_GLOBAL[0],
    DETAIL_GLOBAL[1],
    DETAIL_GLOBAL[2],
    DETAIL_GLOBAL[3],
];

const LOG_KEYS: [Binding; 12] = [
    detail(&[plain('/')], DetailCommand::OpenSearch)
        .help("/ n N", "search, next, previous")
        .hint("/", "search"),
    detail(&[plain('n')], DetailCommand::NextMatch),
    detail(&[plain('N')], DetailCommand::PreviousMatch),
    detail(&[plain('f')], DetailCommand::ToggleFollow)
        .help("f", "toggle follow")
        .hint("f", "follow"),
    detail(&[plain('w')], DetailCommand::ToggleWrap)
        .help("w", "toggle wrap")
        .hint("w", "wrap"),
    detail(&[plain('c')], DetailCommand::ClearView)
        .help("c", "clear the view")
        .hint("c", "clear"),
    detail(UP, DetailCommand::LineUp).help("j k / pgup pgdn", "scroll"),
    detail(DOWN, DetailCommand::LineDown),
    detail(&[Key::Plain(KeyCode::PageUp)], DetailCommand::PageUp),
    detail(&[Key::Plain(KeyCode::PageDown)], DetailCommand::PageDown),
    detail(&[plain('g')], DetailCommand::Top).help("g / G", "top / bottom and follow"),
    detail(&[plain('G')], DetailCommand::Bottom),
];

const SIDEWAYS: [Binding; 2] = [
    detail(&[Key::Plain(KeyCode::Left)], DetailCommand::ScrollLeft).help("← →", "scroll sideways"),
    detail(&[Key::Plain(KeyCode::Right)], DetailCommand::ScrollRight),
];

const SERVICE_DEPLOYS: &[Binding] = &[
    detail(ENTER, DetailCommand::OpenDeployment)
        .help("enter", "deployment details")
        .hint("enter", "details"),
    ROLLBACK,
    DEPLOY,
    detail(UP, DetailCommand::DeployCursorUp).help("j k", "move"),
    detail(DOWN, DetailCommand::DeployCursorDown),
    TAB_KEYS[0].unhinted(),
    TAB_KEYS[1],
    TAB_KEYS[2],
    ARROW_TABS[0],
    ARROW_TABS[1],
    BACK,
    DETAIL_GLOBAL[0],
    DETAIL_GLOBAL[1],
    DETAIL_GLOBAL[2],
    DETAIL_GLOBAL[3],
    RESTART,
    START_STOP,
];

const DEPLOYMENT_VIEW: &[Binding] = &[
    detail(UP, DetailCommand::LineUp)
        .help("j k / pgup pgdn", "scroll build logs")
        .hint("j/k", "scroll"),
    ROLLBACK,
    detail(DOWN, DetailCommand::LineDown),
    detail(&[Key::Plain(KeyCode::PageUp)], DetailCommand::PageUp),
    detail(&[Key::Plain(KeyCode::PageDown)], DetailCommand::PageDown),
    detail(&[plain('g')], DetailCommand::Top)
        .help("g / G", "top / bottom")
        .hint("g/G", "top/bottom"),
    detail(&[plain('G')], DetailCommand::Bottom),
    detail(ESC, DetailCommand::CloseDeployment)
        .help("esc", "back")
        .hint("esc", "back"),
    DETAIL_GLOBAL[0],
    DETAIL_GLOBAL[1],
    DETAIL_GLOBAL[2],
    DETAIL_GLOBAL[3],
    DEPLOY,
    RESTART,
    START_STOP,
];

const LOG_SEARCH: &[Binding] = &[
    detail(ENTER, DetailCommand::SearchApply).hint("enter", "search"),
    detail(ESC, DetailCommand::SearchEscape).hint("esc", "clear / close"),
    detail(BACKSPACE, DetailCommand::SearchErase),
    detail(&[Key::AnyChar], DetailCommand::SearchType),
];

const DATABASE_DETAIL: &[Binding] = &[BACK, START_STOP, GLOBAL[0], GLOBAL[1], GLOBAL[2], GLOBAL[3]];

const HELP: &[Binding] = &[bind(&[Key::Anything], Command::CloseOverlay).hint("any key", "close")];

const PROFILE_PICKER: &[Binding] = &[
    bind(ENTER, Command::PickerChoose).hint("enter", "switch"),
    bind(ESC, Command::CloseOverlay).hint("esc", "close"),
    bind(&[plain('q')], Command::Quit).hint("q", "quit"),
    bind(UP, Command::PickerUp),
    bind(DOWN, Command::PickerDown),
];

const CONFIRM: &[Binding] = &[
    bind(&[plain('y')], Command::ConfirmAction).hint("y", "confirm"),
    bind(
        &[
            plain('n'),
            Key::Plain(KeyCode::Esc),
            Key::Plain(KeyCode::Enter),
        ],
        Command::CancelAction,
    )
    .hint("n / esc", "cancel"),
];

/// The bindings of `mode`. Their order is the status bar's: a narrow bar
/// drops hints from the end. A key listed twice would be shadowed; a test
/// keeps every mode free of that.
pub fn bindings(mode: Mode) -> Vec<Binding> {
    match mode {
        Mode::LoginForm => LOGIN_FORM.to_vec(),
        Mode::LoginNavigating => LOGIN_NAVIGATING.to_vec(),
        Mode::Blocked => BLOCKED.to_vec(),
        Mode::Dashboard(Pane::Projects) => DASHBOARD_PROJECTS.to_vec(),
        Mode::Dashboard(Pane::Resources) => DASHBOARD_RESOURCES.to_vec(),
        Mode::DashboardFilter => DASHBOARD_FILTER.to_vec(),
        Mode::ServiceOverview => SERVICE_OVERVIEW.to_vec(),
        Mode::ServiceLogs { wrap } => {
            let mut all = LOG_KEYS.to_vec();
            all.extend([TAB_KEYS[0].unhinted(), TAB_KEYS[1], TAB_KEYS[2]]);
            // With wrap off the arrows scroll sideways instead of switching tabs.
            if wrap {
                all.extend_from_slice(&ARROW_TABS);
            } else {
                all.extend_from_slice(&SIDEWAYS);
            }
            all.push(BACK);
            all.extend_from_slice(&DETAIL_GLOBAL);
            // Last, so a narrow status bar drops these before the log keys.
            all.extend([DEPLOY, RESTART, START_STOP]);
            all
        }
        Mode::ServiceDeploys => SERVICE_DEPLOYS.to_vec(),
        Mode::DeploymentView => DEPLOYMENT_VIEW.to_vec(),
        Mode::LogSearch => LOG_SEARCH.to_vec(),
        Mode::DatabaseDetail => DATABASE_DETAIL.to_vec(),
        Mode::Help => HELP.to_vec(),
        Mode::ProfilePicker => PROFILE_PICKER.to_vec(),
        Mode::Confirm => CONFIRM.to_vec(),
    }
}

/// The mode of the screen under any overlay.
pub fn screen_mode(app: &App) -> Mode {
    match &app.screen {
        Screen::Login(login) if login.navigating => Mode::LoginNavigating,
        Screen::Login(_) => Mode::LoginForm,
        Screen::Blocked { .. } => Mode::Blocked,
        Screen::Dashboard if app.dashboard.editing_filter.is_some() => Mode::DashboardFilter,
        Screen::Dashboard => Mode::Dashboard(app.dashboard.focus),
        Screen::ServiceDetail(detail) if detail.logs.editing_search() => Mode::LogSearch,
        Screen::ServiceDetail(detail) if detail.deployment_view.is_some() => Mode::DeploymentView,
        Screen::ServiceDetail(detail) => match detail.tab {
            DetailTab::Overview => Mode::ServiceOverview,
            DetailTab::Logs => Mode::ServiceLogs {
                wrap: detail.logs.wrap(),
            },
            DetailTab::Deploys => Mode::ServiceDeploys,
        },
        Screen::DatabaseDetail(_) => Mode::DatabaseDetail,
    }
}

pub fn mode(app: &App) -> Mode {
    match &app.overlay {
        Some(Overlay::Confirm(_)) => Mode::Confirm,
        Some(Overlay::ProfilePicker(_)) => Mode::ProfilePicker,
        Some(Overlay::Help) => Mode::Help,
        None => screen_mode(app),
    }
}

pub fn command_for(mode: Mode, event: &KeyEvent) -> Option<Command> {
    bindings(mode)
        .into_iter()
        .find(|binding| binding.keys.iter().any(|key| key.matches(event)))
        .map(|binding| binding.command)
}

/// What the help overlay lists for the screen under it. On the login screen
/// help opens from navigation, so the form's own keys come first.
pub fn help_entries(app: &App) -> Vec<(&'static str, &'static str)> {
    let mode = match screen_mode(app) {
        // Help describes the whole screen, not just the focused pane.
        Mode::Dashboard(_) => Mode::Dashboard(Pane::Resources),
        other => other,
    };
    let mut all = match mode {
        Mode::LoginNavigating => bindings(Mode::LoginForm),
        _ => Vec::new(),
    };
    all.extend(bindings(mode));

    // Grouped: this screen's keys, then its actions, then the global keys.
    let group = |binding: &Binding| match binding.command {
        Command::Action(_) => 1,
        Command::Refresh | Command::OpenProfiles | Command::OpenHelp | Command::Quit => 2,
        _ => 0,
    };
    all.sort_by_key(group);
    all.into_iter().filter_map(|binding| binding.help).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_MODE: [Mode; 15] = [
        Mode::LoginForm,
        Mode::LoginNavigating,
        Mode::Blocked,
        Mode::Dashboard(Pane::Projects),
        Mode::Dashboard(Pane::Resources),
        Mode::DashboardFilter,
        Mode::ServiceOverview,
        Mode::ServiceLogs { wrap: true },
        Mode::ServiceLogs { wrap: false },
        Mode::ServiceDeploys,
        Mode::DeploymentView,
        Mode::LogSearch,
        Mode::DatabaseDetail,
        Mode::ProfilePicker,
        Mode::Confirm,
    ];

    #[test]
    fn no_key_is_bound_twice_in_a_mode() {
        for mode in EVERY_MODE {
            let keys: Vec<Key> = bindings(mode)
                .iter()
                .flat_map(|binding| binding.keys.iter().copied())
                .filter(|key| !matches!(key, Key::AnyChar | Key::Anything))
                .collect();
            for (index, key) in keys.iter().enumerate() {
                assert!(
                    !keys[index + 1..].contains(key),
                    "{key:?} is bound twice in {mode:?}"
                );
            }
        }
    }

    #[test]
    fn a_text_field_catches_characters_after_its_own_keys() {
        for mode in [Mode::LoginForm, Mode::DashboardFilter, Mode::LogSearch] {
            let all = bindings(mode);
            let catch_all = all
                .iter()
                .position(|binding| binding.keys.contains(&Key::AnyChar))
                .unwrap();
            assert!(all[catch_all + 1..]
                .iter()
                .all(|binding| binding.keys.iter().all(|key| matches!(key, Key::Ctrl(_)))));
        }
    }

    #[test]
    fn every_documented_binding_has_a_key() {
        for mode in EVERY_MODE {
            for binding in bindings(mode) {
                assert!(
                    !binding.keys.is_empty(),
                    "{:?} in {mode:?}",
                    binding.command
                );
            }
        }
    }

    #[test]
    fn the_logs_tab_offers_the_service_actions() {
        let commands: Vec<Command> = bindings(Mode::ServiceLogs { wrap: true })
            .iter()
            .map(|binding| binding.command)
            .collect();
        for key in [ActionKey::Deploy, ActionKey::Restart, ActionKey::StartStop] {
            assert!(commands.contains(&Command::Action(key)));
        }
    }

    #[test]
    fn ctrl_does_not_trigger_a_plain_key() {
        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(command_for(Mode::Dashboard(Pane::Projects), &ctrl_q), None);
    }

    #[test]
    fn uppercase_action_keys_match_with_shift() {
        let shift_d = KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT);
        assert_eq!(
            command_for(Mode::ServiceOverview, &shift_d),
            Some(Command::Action(ActionKey::Deploy))
        );
    }
}
