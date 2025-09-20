# CASTERM Complete Specification v2.0
## Modern Terminal Emulator with Built-in Multiplexer

---

## TABLE OF CONTENTS

1. [PROJECT IDENTITY](#project-identity)
2. [CORE ARCHITECTURE](#core-architecture)
3. [TERMINAL EMULATION](#terminal-emulation)
4. [MULTIPLEXER SYSTEM](#multiplexer-system)
5. [UNIVERSAL STATUS BAR](#universal-status-bar)
6. [BUILT-IN COMMANDS](#built-in-commands)
7. [CONFIGURATION SYSTEM](#configuration-system)
8. [KEY BINDINGS](#key-bindings)
9. [INTERFACE MODES](#interface-modes)
10. [SESSION MANAGEMENT](#session-management)
11. [WINDOW AND PANE MANAGEMENT](#window-and-pane-management)
12. [COPY AND PASTE SYSTEM](#copy-and-paste-system)
13. [BROADCAST MODE](#broadcast-mode)
14. [SEARCH SYSTEM](#search-system)
15. [PROJECT DETECTION AND TEMPLATES](#project-detection-and-templates)
16. [VCS SUPPORT](#vcs-support)
17. [AI INTEGRATION](#ai-integration)
18. [SERVICE INTEGRATION](#service-integration)
19. [FONT SYSTEM](#font-system)
20. [THEME SYSTEM](#theme-system)
21. [COMMAND PALETTE](#command-palette)
22. [COMPLETION ENGINE](#completion-engine)
23. [ERROR HANDLING](#error-handling)
24. [PERFORMANCE](#performance)
25. [PLATFORM SUPPORT](#platform-support)
26. [BUILD SYSTEM](#build-system)
27. [TESTING FRAMEWORK](#testing-framework)
28. [DISTRIBUTION](#distribution)
29. [FILE STRUCTURE](#file-structure)
30. [IMPLEMENTATION ROADMAP](#implementation-roadmap)

---

## PROJECT IDENTITY

```yaml
name: CASTERM
description: Modern terminal emulator with built-in multiplexer
version: 1.0.0
license: MIT
language: Rust 2021 Edition (minimum 1.70)
binary: casterm
repository: https://github.com/casapps/casterm
```

### Mission Statement
Build a zero-configuration terminal emulator that combines the best of modern terminal emulators with a full tmux-like multiplexer, requiring no external dependencies or shell configuration.

### Core Principles
- **Zero Configuration Required** - Works perfectly out of the box
- **No External Dependencies** - Single self-contained binary
- **Universal Status Bar** - Works without shell integration
- **Built-in Everything** - 200+ features from tmux plugins included
- **Cross-Platform** - Windows, macOS, Linux, BSD support
- **Smart Detection** - Automatically adapts to environment

---

## CORE ARCHITECTURE

```rust
pub struct Casterm {
    // Terminal emulation core
    terminal: WezTerminal,
    pty: PtySystem,
    
    // Multiplexer system
    sessions: SessionManager,
    windows: WindowManager,
    panes: PaneManager,
    layouts: LayoutEngine,
    
    // User interface
    interface_mode: InterfaceMode,
    status_bar: UniversalStatusBar,
    command_palette: CommandPalette,
    
    // Built-in features
    builtin_commands: BuiltinCommands,
    completion_engine: CompletionEngine,
    features: FeatureRegistry,
    
    // Developer tools
    vcs_manager: VcsManager,
    project_detector: ProjectDetector,
    ai_assistant: AiAssistant,
    services: ServiceManager,
    
    // System management
    config: Configuration,
    platform: Box<dyn PlatformLayer>,
    error_handler: ErrorHandler,
    performance_monitor: PerformanceMonitor,
}

pub enum InterfaceMode {
    GUI,  // Native window with embedded terminal
    TUI,  // Running in existing terminal
}

impl Casterm {
    pub fn new() -> Result<Self> {
        let platform = PlatformLayer::detect()?;
        let interface_mode = Self::detect_interface_mode(&platform)?;
        let config = Configuration::load_or_create()?;
        
        let terminal = WezTerminal::new()?;
        let pty = PtySystem::new(&platform)?;
        
        Ok(Self {
            terminal,
            pty,
            sessions: SessionManager::new(),
            windows: WindowManager::new(),
            panes: PaneManager::new(),
            layouts: LayoutEngine::new(),
            interface_mode,
            status_bar: UniversalStatusBar::new(&config),
            command_palette: CommandPalette::new(),
            builtin_commands: BuiltinCommands::new(),
            completion_engine: CompletionEngine::new(),
            features: FeatureRegistry::new(),
            vcs_manager: VcsManager::new(),
            project_detector: ProjectDetector::new(),
            ai_assistant: AiAssistant::new(&config),
            services: ServiceManager::new(&config),
            config,
            platform,
            error_handler: ErrorHandler::new(),
            performance_monitor: PerformanceMonitor::new(),
        })
    }
}
```

---

## TERMINAL EMULATION

```rust
pub struct WezTerminal {
    term: wezterm_term::Terminal,
    config: TerminalConfig,
    parser: VteParser,
    scrollback: ScrollbackBuffer,
}

pub struct TerminalConfig {
    rows: u16,
    cols: u16,
    scrollback_lines: usize,
    alternate_scroll_mode: bool,
    bracketed_paste: bool,
    mouse_reporting: MouseReporting,
    true_color: bool,
}

impl WezTerminal {
    pub fn process_input(&mut self, data: &[u8]) -> Result<()> {
        self.parser.process(data);
        
        for action in self.parser.drain_actions() {
            match action {
                VteAction::Print(c) => self.term.print(c),
                VteAction::Execute(b) => self.execute_control(b),
                VteAction::CsiDispatch(params, intermediates, ignore, c) => {
                    self.handle_csi(params, intermediates, ignore, c)?
                },
                VteAction::OscDispatch(params, bell_terminated) => {
                    self.handle_osc(params, bell_terminated)?
                },
                VteAction::EscDispatch(intermediates, ignore, b) => {
                    self.handle_esc(intermediates, ignore, b)?
                },
                _ => {}
            }
        }
        
        Ok(())
    }
    
    pub fn handle_csi(&mut self, params: &[i64], intermediates: &[u8], _ignore: bool, c: char) -> Result<()> {
        match c {
            'A' => self.cursor_up(params.get(0).copied().unwrap_or(1)),
            'B' => self.cursor_down(params.get(0).copied().unwrap_or(1)),
            'C' => self.cursor_forward(params.get(0).copied().unwrap_or(1)),
            'D' => self.cursor_backward(params.get(0).copied().unwrap_or(1)),
            'H' | 'f' => {
                let row = params.get(0).copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                self.cursor_position(row, col);
            },
            'J' => self.erase_in_display(params.get(0).copied().unwrap_or(0)),
            'K' => self.erase_in_line(params.get(0).copied().unwrap_or(0)),
            'm' => self.select_graphic_rendition(params),
            's' => self.save_cursor(),
            'u' => self.restore_cursor(),
            'l' if intermediates == b"?" => self.reset_dec_private_mode(params),
            'h' if intermediates == b"?" => self.set_dec_private_mode(params),
            _ => {}
        }
        Ok(())
    }
}
```

---

## MULTIPLEXER SYSTEM

```rust
pub struct SessionManager {
    sessions: HashMap<SessionId, Session>,
    current_session: Option<SessionId>,
    session_groups: HashMap<String, Vec<SessionId>>,
}

pub struct Session {
    id: SessionId,
    name: String,
    windows: Vec<WindowId>,
    current_window: usize,
    created_at: DateTime<Utc>,
    working_dir: PathBuf,
    environment: HashMap<String, String>,
    
    // Persistent state
    command_history: VecDeque<String>,
    search_history: VecDeque<String>,
    
    // Session features
    auto_save: bool,
    resurrect_state: Option<ResurrectState>,
}

pub struct WindowManager {
    windows: HashMap<WindowId, Window>,
    layouts: LayoutManager,
}

pub struct Window {
    id: WindowId,
    name: String,
    panes: Vec<PaneId>,
    layout: Layout,
    active_pane: PaneId,
    base_index: usize,
}

pub struct PaneManager {
    panes: HashMap<PaneId, Pane>,
    focus_history: VecDeque<PaneId>,
}

pub struct Pane {
    id: PaneId,
    terminal: WezTerminal,
    process: Child,
    working_dir: PathBuf,
    size: PaneSize,
    position: PanePosition,
    scrollback: ScrollbackBuffer,
    
    // Pane state
    zoomed: bool,
    marked: bool,
    synchronized: bool,
    in_copy_mode: bool,
}

#[derive(Debug, Clone)]
pub enum Layout {
    Even,           // Equal size
    MainVertical,   // Main left, others stacked right
    MainHorizontal, // Main top, others stacked bottom
    Grid,           // Grid layout
    Custom(CustomLayout),
}

impl SessionManager {
    pub fn create_session(&mut self, name: Option<String>) -> Result<SessionId> {
        let id = SessionId::new();
        let name = name.unwrap_or_else(|| self.generate_session_name());
        
        let session = Session {
            id,
            name,
            windows: vec![],
            current_window: 0,
            created_at: Utc::now(),
            working_dir: env::current_dir()?,
            environment: env::vars().collect(),
            command_history: VecDeque::with_capacity(1000),
            search_history: VecDeque::with_capacity(100),
            auto_save: true,
            resurrect_state: None,
        };
        
        self.sessions.insert(id, session);
        self.current_session = Some(id);
        
        // Create default window
        self.create_window(id, None)?;
        
        Ok(id)
    }
    
    pub fn attach_session(&mut self, name_or_id: &str) -> Result<()> {
        let session_id = self.find_session(name_or_id)?;
        self.current_session = Some(session_id);
        Ok(())
    }
    
    pub fn detach_session(&mut self) -> Result<()> {
        self.current_session = None;
        Ok(())
    }
}
```

---

## UNIVERSAL STATUS BAR

```rust
pub struct UniversalStatusBar {
    // Mode indicator always far left
    mode_indicator: ModeIndicator,
    
    // Layout sections
    left: Vec<StatusComponent>,
    center: Vec<StatusComponent>,
    right: Vec<StatusComponent>,
    
    // Direct detection (no shell integration)
    shell_detector: ShellDetector,
    vcs_detector: VcsDetector,
    language_detector: LanguageDetector,
    env_detector: EnvironmentDetector,
    
    // Settings
    position: StatusBarPosition,
    auto_hide: bool,
    refresh_interval: Duration,
}

#[derive(Debug, Clone)]
pub enum StatusComponent {
    // Mode (always far left)
    ModeIndicator,
    
    // Session info
    SessionName,
    WindowList,
    WindowNumber,
    PaneNumber,
    
    // System info
    Hostname,
    Username,
    UserAtHost,
    
    // Shell info
    Shell { name: String, version: String },
    
    // Directory
    WorkingDirectory { path: PathBuf, abbreviated: bool },
    
    // VCS (all 20+ types)
    VcsStatus { 
        vcs_type: VcsType,
        branch: String,
        status: VcsStatusInfo,
    },
    
    // Development
    Languages { detected: Vec<Language> },
    VirtualEnv { name: String, type_: EnvType },
    
    // Command info
    LastExitCode { code: i32 },
    CommandTimer { duration: Duration },
    
    // System
    DateTime { format: String },
    Uptime,
    LoadAverage,
    Memory,
    Battery,
    
    // Features
    ZoomIndicator,
    BroadcastIndicator { count: u32 },
}

#[derive(Debug, Clone)]
pub enum Mode {
    Normal,         // No indicator
    Prefix,         // After prefix key pressed
    Copy,           // Copy/selection mode
    Broadcast(u32), // Broadcasting to N panes
    Zen,            // Zen mode active
    Command,        // Command prompt
    Search,         // Search mode
}

impl UniversalStatusBar {
    pub fn detect_all(&mut self) -> Result<()> {
        // Direct detection without shell integration
        self.detect_shell()?;
        self.detect_vcs()?;
        self.detect_languages()?;
        self.detect_virtual_env()?;
        self.detect_command_status()?;
        Ok(())
    }
    
    fn detect_shell(&mut self) -> Result<()> {
        // Direct detection via /proc or ps
        #[cfg(target_os = "linux")]
        {
            if let Ok(cmdline) = fs::read_to_string("/proc/self/cmdline") {
                let shell = self.parse_shell_from_cmdline(&cmdline);
                self.shell = Some(shell);
            }
        }
        
        #[cfg(target_os = "macos")]
        {
            let ppid = unsafe { libc::getppid() };
            let output = Command::new("ps")
                .args(&["-p", &ppid.to_string(), "-o", "comm="])
                .output()?;
            let shell = String::from_utf8_lossy(&output.stdout).trim().to_string();
            self.shell = Some(shell);
        }
        
        Ok(())
    }
    
    fn detect_vcs(&mut self) -> Result<()> {
        // Check all 20+ VCS types
        let vcs_types = [
            (VcsType::Git, ".git"),
            (VcsType::Mercurial, ".hg"),
            (VcsType::Subversion, ".svn"),
            (VcsType::Fossil, ".fossil"),
            (VcsType::Bazaar, ".bzr"),
            (VcsType::Darcs, "_darcs"),
            (VcsType::Pijul, ".pijul"),
            (VcsType::CVS, "CVS"),
            (VcsType::Perforce, ".p4config"),
            (VcsType::ClearCase, ".clearcase"),
            (VcsType::TFS, "$tf"),
            (VcsType::Plastic, ".plastic"),
            (VcsType::ArX, ".arx"),
            (VcsType::Monotone, "_MTN"),
            (VcsType::SCCS, "SCCS"),
            (VcsType::RCS, "RCS"),
            (VcsType::BitKeeper, "BitKeeper"),
            (VcsType::Aegis, ".aegisrc"),
            (VcsType::AccuRev, ".accurev"),
            (VcsType::SourceSafe, "vssver.scc"),
        ];
        
        for (vcs_type, marker) in &vcs_types {
            if self.find_vcs_root(marker).is_some() {
                self.current_vcs = Some(*vcs_type);
                self.detect_vcs_status(*vcs_type)?;
                break;
            }
        }
        
        Ok(())
    }
    
    pub fn render(&self, width: u16) -> String {
        // Mode indicator always first
        let mode_str = self.mode_indicator.render(&self.current_mode);
        let mode_width = if mode_str.is_empty() { 0 } else { mode_str.len() + 3 };
        
        let remaining_width = width.saturating_sub(mode_width as u16);
        
        let left = self.render_components(&self.left);
        let center = self.render_components(&self.center);
        let right = self.render_components(&self.right);
        
        if mode_str.is_empty() {
            self.layout_three_sections(left, center, right, width)
        } else {
            format!("{} | {}", 
                mode_str,
                self.layout_three_sections(left, center, right, remaining_width)
            )
        }
    }
}
```

---

## BUILT-IN COMMANDS

```rust
pub struct BuiltinCommands {
    commands: HashMap<String, Box<dyn BuiltinCommand>>,
    printf: PrintfFunctions,
}

pub trait BuiltinCommand: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, args: &[String]) -> Result<String>;
}

// Printf functions for colored output
pub struct PrintfFunctions {
    use_color: bool,
    colors: ColorScheme,
}

impl PrintfFunctions {
    pub fn printf_red(&self, msg: &str) {
        println!("{}{}{}", self.colors.red, msg, self.colors.reset);
    }
    
    pub fn printf_green(&self, msg: &str) {
        println!("{}{}{}", self.colors.green, msg, self.colors.reset);
    }
    
    pub fn printf_yellow(&self, msg: &str) {
        println!("{}{}{}", self.colors.yellow, msg, self.colors.reset);
    }
    
    pub fn printf_blue(&self, msg: &str) {
        println!("{}{}{}", self.colors.blue, msg, self.colors.reset);
    }
    
    pub fn printf_purple(&self, msg: &str) {
        println!("{}{}{}", self.colors.purple, msg, self.colors.reset);
    }
    
    pub fn printf_cyan(&self, msg: &str) {
        println!("{}{}{}", self.colors.cyan, msg, self.colors.reset);
    }
    
    pub fn printf_exit(&self, msg: &str, code: i32) -> ! {
        self.printf_red(msg);
        std::process::exit(code);
    }
}

// Auto-tail command
pub struct AutoTailCommand;

impl BuiltinCommand for AutoTailCommand {
    fn name(&self) -> &str { "auto_tail" }
    
    fn description(&self) -> &str { 
        "Automatically tail log files in directories"
    }
    
    fn execute(&self, args: &[String]) -> Result<String> {
        let mut watcher = LogWatcher::new();
        let mut dirs = Vec::new();
        
        for arg in args {
            let path = PathBuf::from(arg);
            if !path.exists() {
                // Wait for directory to exist
                while !path.exists() {
                    thread::sleep(Duration::from_secs(2));
                }
            }
            dirs.push(path);
        }
        
        if dirs.is_empty() {
            dirs.push(dirs::home_dir().unwrap().join(".local/log"));
        }
        
        watcher.watch_directories(dirs)?;
        watcher.start_tail()
    }
}

// Cheat.sh command
pub struct CheatShCommand;

impl BuiltinCommand for CheatShCommand {
    fn name(&self) -> &str { "cheat" }
    
    fn description(&self) -> &str { 
        "Get programming cheat sheets from cheat.sh"
    }
    
    fn execute(&self, args: &[String]) -> Result<String> {
        let query = args.join("/");
        let url = format!("https://cheat.sh/{}", query);
        
        let response = reqwest::blocking::get(&url)?.text()?;
        Ok(response)
    }
}

// DevHints command
pub struct DevHintsCommand;

impl BuiltinCommand for DevHintsCommand {
    fn name(&self) -> &str { "devhints" }
    
    fn description(&self) -> &str {
        "Quick reference for developer tools"
    }
    
    fn execute(&self, args: &[String]) -> Result<String> {
        let topic = args.join("-");
        Ok(format!("https://devhints.io/{}", topic))
    }
}

// TLDR command
pub struct TldrCommand;

impl BuiltinCommand for TldrCommand {
    fn name(&self) -> &str { "tldr" }
    
    fn description(&self) -> &str {
        "Simplified man pages with practical examples"
    }
    
    fn execute(&self, args: &[String]) -> Result<String> {
        let command = args.first().ok_or("Specify a command")?;
        let url = format!(
            "https://raw.githubusercontent.com/tldr-pages/tldr/main/pages/common/{}.md", 
            command
        );
        
        let response = reqwest::blocking::get(&url)?;
        if response.status().is_success() {
            let content = response.text()?;
            Ok(self.parse_tldr_markdown(content))
        } else {
            Err(anyhow!("Command not found: {}", command))
        }
    }
    
    fn parse_tldr_markdown(&self, md: String) -> String {
        md.lines()
            .map(|line| {
                if line.starts_with('#') {
                    line.replace('#', "").trim().to_string()
                } else if line.starts_with('`') && line.ends_with('`') {
                    format!("  {}", line.trim_matches('`'))
                } else if line.starts_with('-') {
                    format!("• {}", &line[1..].trim())
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// Urban Dictionary command
pub struct UrbanDictCommand;

impl BuiltinCommand for UrbanDictCommand {
    fn name(&self) -> &str { "urbandict" }
    
    fn description(&self) -> &str {
        "Look up slang definitions from Urban Dictionary"
    }
    
    fn execute(&self, args: &[String]) -> Result<String> {
        let term = args.join(" ");
        let url = format!(
            "https://api.urbandictionary.com/v0/define?term={}", 
            urlencoding::encode(&term)
        );
        
        let response: serde_json::Value = reqwest::blocking::get(&url)?.json()?;
        
        if let Some(list) = response["list"].as_array() {
            if let Some(first) = list.first() {
                let definition = first["definition"].as_str().unwrap_or("No definition");
                let example = first["example"].as_str().unwrap_or("");
                
                return Ok(format!(
                    "{}: {}\n\nExample: {}",
                    term, definition, example
                ));
            }
        }
        
        Err(anyhow!("No definition found for: {}", term))
    }
}

// Thesaurus command
pub struct ThesaurusCommand;

impl BuiltinCommand for ThesaurusCommand {
    fn name(&self) -> &str { "thesaurus" }
    
    fn description(&self) -> &str {
        "Find synonyms and antonyms"
    }
    
    fn execute(&self, args: &[String]) -> Result<String> {
        let word = args.first().ok_or("Specify a word")?;
        
        let mut stream = TcpStream::connect("dict.org:2628")?;
        let query = format!("DEFINE moby-thesaurus {}\r\n", word);
        stream.write_all(query.as_bytes())?;
        
        let mut buffer = String::new();
        let mut reader = BufReader::new(&stream);
        
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            
            if line.starts_with("250") {
                break;
            }
            if line.starts_with("552") {
                return Err(anyhow!("No entry for: {}", word));
            }
            
            buffer.push_str(&line);
        }
        
        stream.write_all(b"QUIT\r\n")?;
        Ok(buffer)
    }
}

impl BuiltinCommands {
    pub fn new() -> Self {
        let mut commands = HashMap::new();
        
        // Register all commands
        commands.insert("auto_tail".to_string(), Box::new(AutoTailCommand) as Box<dyn BuiltinCommand>);
        commands.insert("cheat".to_string(), Box::new(CheatShCommand) as Box<dyn BuiltinCommand>);
        commands.insert("devhints".to_string(), Box::new(DevHintsCommand) as Box<dyn BuiltinCommand>);
        commands.insert("tldr".to_string(), Box::new(TldrCommand) as Box<dyn BuiltinCommand>);
        commands.insert("urbandict".to_string(), Box::new(UrbanDictCommand) as Box<dyn BuiltinCommand>);
        commands.insert("thesaurus".to_string(), Box::new(ThesaurusCommand) as Box<dyn BuiltinCommand>);
        
        Self {
            commands,
            printf: PrintfFunctions::new(true),
        }
    }
    
    pub fn execute(&self, cmd: &str, args: &[String]) -> Result<String> {
        self.commands
            .get(cmd)
            .ok_or_else(|| anyhow!("Command not found: {}", cmd))?
            .execute(args)
    }
}
```

---

## CONFIGURATION SYSTEM

```yaml
# ~/.config/casterm/custom.yml
version: "1.0"

# ============================================================================
# MULTIPLEXER
# ============================================================================
multiplexer:
  prefix_key: C-b                    # Key combination before commands

# ============================================================================
# SESSION STARTUP
# ============================================================================
startup:
  # Option 1: Simple count
  windows: 3
  
  # Option 2: Named windows with commands
  windows:
    monitor: htop
    logs: tail -f /var/log/syslog
    editor: vim
    server: ssh prod-server
  
  # Option 3: Use template
  template: node
  
  # Option 4: Template with overrides
  template: docker
  overrides:
    build: make build

# ============================================================================
# DISPLAY SETTINGS
# ============================================================================
display:
  font: "Source Code Pro Nerd Font"
  font_size: 14.0
  line_spacing: 1.2
  letter_spacing: 0.0
  transparency: 15                    # 0-100% or 0.0-1.0
  theme: "dracula"
  blur_behind_window: false

# ============================================================================
# TERMINAL BEHAVIOR
# ============================================================================
terminal:
  shell: "auto"
  scrollback_lines: 10000
  cursor_shape: "beam"                # beam, block, or underline
  cursor_blinks: true
  cursor_blink_speed: 500
  hide_cursor_when_typing: true

# ============================================================================
# STATUS BAR
# ============================================================================
status_bar:
  position: "bottom"
  always_visible: true
  minimal_in_apps:
    - "vim"
    - "nvim"
    - "htop"
  
  segments:
    mode_indicator: true
    session_name: true
    window_list: false
    shell_version: true
    working_directory: true
    vcs_status: true
    language_detection: true
    virtual_environments: true
    command_timer: true
    last_exit_code: true
    date_time: true
    uptime: true
    weather: false
    battery: false
  
  datetime_format: "%m/%d %H:%M"
  
  colors:
    background: "default"
    text: "default"
    mode_prefix: "magenta"
    mode_copy: "yellow"
    mode_broadcast: "red"

# ============================================================================
# AI ASSISTANT
# ============================================================================
ai_assistant:
  ollama_server: "http://localhost:11434"
  model: "auto"
  history_limit: 1000

# ============================================================================
# QUICK UPLOAD SERVICES
# ============================================================================
quick_upload:
  pastebin: "dpaste"
  url_shortener: "isgd"

# ============================================================================
# KEYBOARD SHORTCUTS
# ============================================================================
shortcuts:
  # File operations
  new_tab: "Ctrl+T"
  new_window: "Ctrl+N"
  close_tab: "Ctrl+W"
  close_window: "Ctrl+Q"
  
  # Pane management
  split_pane_right: "Ctrl+D"
  split_pane_down: "Ctrl+Shift+D"
  close_pane: "Ctrl+Shift+Q"
  navigate_up: "Ctrl+Shift+Up"
  navigate_down: "Ctrl+Shift+Down"
  navigate_left: "Ctrl+Shift+Left"
  navigate_right: "Ctrl+Shift+Right"
  
  # View operations
  zoom_in: "Ctrl+="
  zoom_out: "Ctrl+-"
  reset_zoom: "Ctrl+0"
  fullscreen: "F11"
  zen_mode: "Ctrl+Shift+Z"
  
  # Text operations
  copy_text: "Ctrl+Shift+C"
  paste_text: "Ctrl+Shift+V"
  select_all: "Ctrl+Shift+A"
  find_text: "Ctrl+Shift+F"
  
  # Special features
  command_palette: "Ctrl+P"
  settings: "Ctrl+,"

# ============================================================================
# COPY AND PASTE
# ============================================================================
copy_paste:
  mouse_selection_copies: true
  middle_click_paste: true
  confirm_large_paste: true
  strip_trailing_whitespace: false
  bracket_paste_mode: true

# ============================================================================
# MOUSE SUPPORT
# ============================================================================
mouse:
  enabled: true
  hide_when_typing: false
  scroll_speed: 3

# ============================================================================
# BROADCAST MODE
# ============================================================================
broadcast:
  confirm_enable: true
  visual_indicator: "red_border"
  exclude_local_by_default: false

# ============================================================================
# SEARCH
# ============================================================================
search:
  case_sensitive: false
  wrap_search: true
  use_regex: false
  incremental: true

# ============================================================================
# PERFORMANCE
# ============================================================================
performance:
  gpu_acceleration: "auto"
  max_fps: 60
  lazy_rendering: true
```

```rust
pub struct Configuration {
    pub multiplexer: MultiplexerConfig,
    pub startup: StartupConfig,
    pub display: DisplayConfig,
    pub terminal: TerminalConfig,
    pub status_bar: StatusBarConfig,
    pub ai_assistant: AiConfig,
    pub services: ServicesConfig,
    pub shortcuts: KeyBindings,
    pub copy_paste: CopyPasteConfig,
    pub mouse: MouseConfig,
    pub broadcast: BroadcastConfig,
    pub search: SearchConfig,
    pub performance: PerformanceConfig,
}

impl Configuration {
    pub fn load_or_create() -> Result<Self> {
        let config_path = Self::get_config_path()?;
        
        if config_path.exists() {
            let yaml = fs::read_to_string(&config_path)?;
            Ok(serde_yaml::from_str(&yaml)?)
        } else {
            let default_config = Self::default();
            let yaml = serde_yaml::to_string(&default_config)?;
            
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            fs::write(&config_path, yaml)?;
            Ok(default_config)
        }
    }
    
    fn get_config_path() -> Result<PathBuf> {
        // Search order for configuration
        let search_paths = vec![
            PathBuf::from("./.casmux.yml"),
            PathBuf::from("./.casmux.yaml"),
            dirs::config_dir().unwrap().join("casterm/custom.yml"),
            dirs::config_dir().unwrap().join("casterm/custom.yaml"),
        ];
        
        for path in &search_paths {
            if path.exists() {
                return Ok(path.clone());
            }
        }
        
        // Default path if none exist
        Ok(dirs::config_dir().unwrap().join("casterm/custom.yml"))
    }
}
```

---

## KEY BINDINGS

```rust
pub struct KeyBindingManager {
    bindings: HashMap<KeyBinding, Action>,
    prefix_key: KeyModifiers,
    prefix_active: bool,
    copy_mode: CopyMode,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct KeyBinding {
    key: KeyCode,
    modifiers: KeyModifiers,
    with_prefix: bool,
}

#[derive(Debug, Clone)]
pub enum Action {
    // Session management
    DetachSession,
    ShowSessionPicker,
    RenameSession,
    SwitchLastSession,
    
    // Window management
    NewWindow,
    NextWindow,
    PreviousWindow,
    LastWindow,
    ShowWindowPicker,
    KillWindow,
    RenameWindow,
    SelectWindow(u8),
    
    // Pane management
    SplitHorizontal,
    SplitVertical,
    NavigatePane(Direction),
    ResizePane(Direction, i32),
    KillPane,
    ZoomPane,
    BreakPane,
    JoinPane,
    SwapPaneUp,
    SwapPaneDown,
    NextLayout,
    RotatePanes,
    
    // Copy mode
    EnterCopyMode,
    PasteBuffer,
    ShowBuffers,
    ListBuffers,
    
    // Special features
    ToggleBroadcast,
    ToggleMouse,
    ReloadConfig,
    ShowKeybindings,
    CommandPrompt,
    ShowClock,
    DisplayMessage,
    
    // Global (no prefix)
    ShowHelp,
    OpenCommandPalette,
    GlobalSearch,
    ToggleZenMode,
    FontDecrease,
    FontIncrease,
    FontReset,
    NavigatePaneOrVim(Direction),
}

impl KeyBindingManager {
    pub fn default_bindings() -> HashMap<KeyBinding, Action> {
        let mut bindings = HashMap::new();
        
        // Global keys (no prefix needed)
        bindings.insert(
            KeyBinding { key: KeyCode::F1, modifiers: KeyModifiers::NONE, with_prefix: false },
            Action::ShowHelp
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('p'), modifiers: KeyModifiers::CONTROL, with_prefix: false },
            Action::OpenCommandPalette
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('f'), modifiers: KeyModifiers::CONTROL, with_prefix: false },
            Action::GlobalSearch
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('z'), modifiers: KeyModifiers::CONTROL, with_prefix: false },
            Action::ToggleZenMode
        );
        
        // Prefix + key bindings
        bindings.insert(
            KeyBinding { key: KeyCode::Char('d'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::DetachSession
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('s'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::ShowSessionPicker
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('c'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::NewWindow
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('n'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::NextWindow
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('p'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::PreviousWindow
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('\\'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::SplitHorizontal
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('/'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::SplitVertical
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('x'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::KillPane
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('z'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::ZoomPane
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('['), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::EnterCopyMode
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char(']'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::PasteBuffer
        );
        bindings.insert(
            KeyBinding { key: KeyCode::Char('B'), modifiers: KeyModifiers::NONE, with_prefix: true },
            Action::ToggleBroadcast
        );
        
        bindings
    }
    
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        let binding = KeyBinding {
            key: key.code,
            modifiers: key.modifiers,
            with_prefix: self.prefix_active,
        };
        
        // Check for prefix key
        if !self.prefix_active && self.is_prefix_key(&binding) {
            self.prefix_active = true;
            self.start_prefix_timeout();
            return Ok(None);
        }
        
        // Look up action
        if let Some(action) = self.bindings.get(&binding) {
            self.prefix_active = false;
            return Ok(Some(action.clone()));
        }
        
        // Pass through if no binding
        Ok(None)
    }
}
```

---

## INTERFACE MODES

```rust
pub enum InterfaceMode {
    GUI(GuiMode),
    TUI(TuiMode),
}

pub struct GuiMode {
    window: Window,
    event_loop: EventLoop<()>,
    renderer: Renderer,
}

pub struct TuiMode {
    terminal: CrosstermTerminal,
    event_reader: EventReader,
}

impl InterfaceMode {
    pub fn detect() -> Result<Self> {
        // Check if in SSH/Mosh session
        if env::var("SSH_CONNECTION").is_ok() || 
           env::var("SSH_TTY").is_ok() ||
           env::var("MOSH_CONNECTION").is_ok() {
            return Ok(InterfaceMode::TUI(TuiMode::new()?));
        }
        
        // Check if no display available
        #[cfg(unix)]
        if env::var("DISPLAY").is_err() && env::var("WAYLAND_DISPLAY").is_err() {
            return Ok(InterfaceMode::TUI(TuiMode::new()?));
        }
        
        // Check if running in terminal
        if atty::is(atty::Stream::Stdout) {
            if let Some(parent) = get_parent_process() {
                if is_terminal_emulator(&parent) {
                    return Ok(InterfaceMode::TUI(TuiMode::new()?));
                }
            }
        }
        
        // Default to GUI
        Ok(InterfaceMode::GUI(GuiMode::new()?))
    }
}

impl GuiMode {
    pub fn new() -> Result<Self> {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("CASTERM")
            .with_inner_size(LogicalSize::new(1200, 800))
            .build(&event_loop)?;
        
        let renderer = Renderer::new(&window)?;
        
        Ok(Self {
            window,
            event_loop,
            renderer,
        })
    }
    
    pub fn run(mut self, mut app: Casterm) -> Result<()> {
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;
            
            match event {
                Event::WindowEvent { event, .. } => {
                    app.handle_window_event(event);
                }
                Event::MainEventsCleared => {
                    app.update();
                    self.renderer.render(&app);
                    self.window.request_redraw();
                }
                _ => {}
            }
        })
    }
}

impl TuiMode {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        
        Ok(Self {
            terminal,
            event_reader: EventReader::new(),
        })
    }
    
    pub fn run(mut self, mut app: Casterm) -> Result<()> {
        loop {
            self.terminal.draw(|f| app.render_tui(f))?;
            
            if let Some(event) = self.event_reader.next()? {
                if !app.handle_event(event)? {
                    break;
                }
            }
        }
        
        self.cleanup()?;
        Ok(())
    }
    
    fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        Ok(())
    }
}
```

---

## SESSION MANAGEMENT

```rust
pub struct SessionManager {
    sessions: HashMap<SessionId, Session>,
    current_session: Option<SessionId>,
    session_groups: HashMap<String, Vec<SessionId>>,
    auto_save: AutoSave,
    resurrect: ResurrectManager,
}

pub struct Session {
    id: SessionId,
    name: String,
    windows: Vec<WindowId>,
    current_window: usize,
    created_at: DateTime<Utc>,
    working_dir: PathBuf,
    environment: HashMap<String, String>,
    
    // Persistent state
    command_history: VecDeque<String>,
    search_history: VecDeque<String>,
    clipboard_history: VecDeque<String>,
    
    // Session features
    auto_save: bool,
    resurrect_state: Option<ResurrectState>,
    
    // Nested session detection
    context: SessionContext,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionContext {
    Local,
    LocalNested(u8),
    RemoteSSH,
    RemoteNested(String, u8),
    Container(String),
    ContainerNested(String, u8),
}

impl SessionContext {
    pub fn detect() -> Self {
        if let Ok(level) = env::var("CASTERM_LEVEL") {
            let depth = level.parse::<u8>().unwrap_or(1);
            
            if env::var("SSH_CONNECTION").is_ok() {
                let host = hostname::get()
                    .map(|h| h.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "remote".to_string());
                return SessionContext::RemoteNested(host, depth);
            }
            
            if Path::new("/.dockerenv").exists() {
                let container = env::var("CONTAINER_NAME")
                    .unwrap_or_else(|_| "container".to_string());
                return SessionContext::ContainerNested(container, depth);
            }
            
            return SessionContext::LocalNested(depth);
        }
        
        if env::var("SSH_CONNECTION").is_ok() {
            SessionContext::RemoteSSH
        } else if Path::new("/.dockerenv").exists() {
            let name = env::var("CONTAINER_NAME")
                .unwrap_or_else(|_| "container".to_string());
            SessionContext::Container(name)
        } else {
            SessionContext::Local
        }
    }
    
    pub fn get_prefix_key(&self) -> String {
        match self {
            SessionContext::Local => "C-b".to_string(),
            SessionContext::LocalNested(depth) | 
            SessionContext::RemoteNested(_, depth) |
            SessionContext::ContainerNested(_, depth) => {
                match depth {
                    1 => "C-a".to_string(),
                    2 => "C-s".to_string(),
                    _ => format!("C-{}", depth + 1),
                }
            }
            _ => "C-b".to_string(),
        }
    }
}

pub struct ResurrectManager {
    save_dir: PathBuf,
    auto_save_interval: Duration,
    last_save: Option<Instant>,
}

impl ResurrectManager {
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let state = ResurrectState {
            name: session.name.clone(),
            windows: self.capture_windows(session)?,
            environment: session.environment.clone(),
            working_dir: session.working_dir.clone(),
            timestamp: Utc::now(),
        };
        
        let path = self.save_dir.join(format!(
            "{}-{}.resurrect",
            session.name,
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        
        let data = bincode::serialize(&state)?;
        fs::write(&path, data)?;
        
        Ok(())
    }
    
    pub fn restore_session(&self, path: &Path) -> Result<Session> {
        let data = fs::read(path)?;
        let state: ResurrectState = bincode::deserialize(&data)?;
        
        let mut session = Session::new(state.name);
        session.environment = state.environment;
        session.working_dir = state.working_dir;
        
        for window_state in state.windows {
            self.restore_window(&mut session, window_state)?;
        }
        
        Ok(session)
    }
}
```

---

## WINDOW AND PANE MANAGEMENT

```rust
pub struct WindowManager {
    windows: HashMap<WindowId, Window>,
    layout_manager: LayoutManager,
    auto_rename: bool,
}

pub struct Window {
    id: WindowId,
    name: String,
    panes: Vec<PaneId>,
    layout: Layout,
    active_pane: PaneId,
    base_index: usize,
    
    // Window features
    locked: bool,
    grouped: Option<WindowGroupId>,
    tags: HashSet<String>,
    notes: String,
}

pub struct PaneManager {
    panes: HashMap<PaneId, Pane>,
    focus_history: VecDeque<PaneId>,
    zoom_state: Option<ZoomState>,
}

pub struct Pane {
    id: PaneId,
    terminal: WezTerminal,
    process: Child,
    working_dir: PathBuf,
    size: PaneSize,
    position: PanePosition,
    scrollback: ScrollbackBuffer,
    
    // Pane state
    zoomed: bool,
    marked: bool,
    synchronized: bool,
    in_copy_mode: bool,
    broadcast_target: bool,
}

#[derive(Debug, Clone)]
pub enum Layout {
    Even,
    MainVertical,
    MainHorizontal,
    Grid,
    Custom(CustomLayout),
}

impl WindowManager {
    pub fn create_window(&mut self, session_id: SessionId, name: Option<String>) -> Result<WindowId> {
        let id = WindowId::new();
        let name = name.unwrap_or_else(|| self.generate_window_name());
        
        let window = Window {
            id,
            name,
            panes: vec![],
            layout: Layout::Even,
            active_pane: PaneId::new(),
            base_index: self.get_next_index(),
            locked: false,
            grouped: None,
            tags: HashSet::new(),
            notes: String::new(),
        };
        
        self.windows.insert(id, window);
        Ok(id)
    }
    
    pub fn split_pane(&mut self, window_id: WindowId, direction: SplitDirection) -> Result<PaneId> {
        let window = self.windows.get_mut(&window_id)?;
        let new_pane_id = self.pane_manager.create_pane()?;
        
        window.panes.push(new_pane_id);
        self.layout_manager.adjust_for_split(&mut window.layout, direction);
        
        Ok(new_pane_id)
    }
    
    pub fn auto_rename_window(&mut self, window_id: WindowId) -> Result<()> {
        if !self.auto_rename {
            return Ok(());
        }
        
        let window = self.windows.get_mut(&window_id)?;
        if let Some(pane) = self.pane_manager.get_active_pane(window) {
            if let Some(process) = pane.get_foreground_process() {
                window.name = match process.name.as_str() {
                    "vim" | "nvim" => {
                        process.args.get(1)
                            .and_then(|f| Path::new(f).file_name())
                            .map(|f| format!("vim:{}", f.to_string_lossy()))
                            .unwrap_or_else(|| "vim".to_string())
                    },
                    "ssh" => {
                        process.args.get(1)
                            .map(|h| format!("ssh:{}", h))
                            .unwrap_or_else(|| "ssh".to_string())
                    },
                    _ => process.name.clone(),
                };
            }
        }
        
        Ok(())
    }
}

impl PaneManager {
    pub fn zoom_pane(&mut self, pane_id: PaneId) -> Result<()> {
        if let Some(zoom) = &self.zoom_state {
            if zoom.pane_id == pane_id {
                self.restore_layout()?;
                self.zoom_state = None;
                return Ok(());
            }
        }
        
        let saved_layout = self.save_current_layout();
        self.zoom_state = Some(ZoomState {
            pane_id,
            saved_layout,
        });
        
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.size = PaneSize::Full;
            pane.position = PanePosition::Origin;
            pane.zoomed = true;
        }
        
        for (&id, pane) in self.panes.iter_mut() {
            if id != pane_id {
                pane.size = PaneSize::Hidden;
            }
        }
        
        Ok(())
    }
}
```

---

## COPY AND PASTE SYSTEM

```rust
pub struct CopyPasteManager {
    selection: Option<Selection>,
    selection_mode: SelectionMode,
    system_clipboard: ClipboardManager,
    primary_selection: Option<ClipboardManager>,
    clipboard_history: VecDeque<ClipboardEntry>,
    max_history: usize,
    
    // Settings
    mouse_enabled: bool,
    auto_copy: bool,
    confirm_large_paste: bool,
    bracket_paste_mode: bool,
}

#[derive(Debug, Clone)]
pub enum SelectionMode {
    Character,
    Word,
    Line,
    Block,
    Semantic,
}

impl CopyPasteManager {
    pub fn handle_mouse_event(&mut self, event: MouseEvent) -> Result<()> {
        if !self.mouse_enabled {
            return Ok(());
        }
        
        match event {
            MouseEvent::Down { x, y, button: MouseButton::Left } => {
                self.start_selection(x, y);
            }
            MouseEvent::Drag { x, y } => {
                self.update_selection(x, y);
                self.highlight_selection();
            }
            MouseEvent::Up { button: MouseButton::Left } => {
                if self.auto_copy {
                    self.copy_selection()?;
                }
            }
            MouseEvent::DoubleClick { x, y } => {
                self.select_word_at(x, y);
                if self.auto_copy {
                    self.copy_selection()?;
                }
            }
            MouseEvent::TripleClick { x, y } => {
                self.select_line_at(x, y);
                if self.auto_copy {
                    self.copy_selection()?;
                }
            }
            MouseEvent::Down { button: MouseButton::Middle, .. } => {
                self.paste_primary()?;
            }
            _ => {}
        }
        
        Ok(())
    }
    
    pub fn copy_selection(&mut self) -> Result<()> {
        if let Some(selection) = &self.selection {
            let text = self.get_selected_text(selection)?;
            let clean_text = self.clean_text(&text);
            
            self.add_to_history(clean_text.clone());
            self.system_clipboard.set_text(clean_text.clone())?;
            
            #[cfg(target_os = "linux")]
            if let Some(primary) = &mut self.primary_selection {
                primary.set_text(clean_text)?;
            }
            
            self.clear_selection();
            self.show_notification("Copied to clipboard");
        }
        
        Ok(())
    }
    
    pub fn paste(&mut self) -> Result<()> {
        let text = self.system_clipboard.get_text()?;
        self.paste_text(text)
    }
    
    pub fn paste_text(&mut self, text: String) -> Result<()> {
        if self.should_confirm_paste(&text) {
            if !self.confirm_paste_dialog(&text)? {
                return Ok(());
            }
        }
        
        let text_to_paste = if self.bracket_paste_mode {
            format!("\x1b[200~{}\x1b[201~", text)
        } else {
            text
        };
        
        self.send_to_active_pane(text_to_paste)?;
        Ok(())
    }
    
    fn should_confirm_paste(&self, text: &str) -> bool {
        if !self.confirm_large_paste {
            return false;
        }
        
        let line_count = text.lines().count();
        if line_count > 5 {
            return true;
        }
        
        const DANGEROUS_PATTERNS: &[&str] = &[
            "rm -rf",
            "sudo",
            "curl | sh",
            "wget | sh",
            ":(){ :|:& };:",
        ];
        
        DANGEROUS_PATTERNS.iter().any(|pattern| text.contains(pattern))
    }
}

pub struct SmartSelection;

impl SmartSelection {
    pub fn select_semantic_unit(&self, x: u16, y: u16) -> Selection {
        let text = self.get_line_at(y);
        let pos = x as usize;
        
        if let Some(sel) = self.select_url_at(&text, pos) {
            return sel;
        }
        
        if let Some(sel) = self.select_file_path_at(&text, pos) {
            return sel;
        }
        
        if let Some(sel) = self.select_ip_address_at(&text, pos) {
            return sel;
        }
        
        if let Some(sel) = self.select_git_hash_at(&text, pos) {
            return sel;
        }
        
        if let Some(sel) = self.select_quoted_text_at(&text, pos) {
            return sel;
        }
        
        self.select_word_at(&text, pos)
    }
}
```

---

## BROADCAST MODE

```rust
pub struct BroadcastManager {
    enabled: bool,
    target_panes: HashSet<PaneId>,
    visual_indicators: BroadcastVisuals,
    safety_config: BroadcastSafety,
    
    // Smart detection
    ssh_panes: HashSet<PaneId>,
    local_panes: HashSet<PaneId>,
}

pub struct BroadcastVisuals {
    border_color: Color,
    border_style: BorderStyle,
    status_indicator: String,
    pane_icon: String,
    flash_on_input: bool,
}

pub struct BroadcastSafety {
    confirm_enable: bool,
    exclude_local_by_default: bool,
    auto_disable_on_error: bool,
    visual_warning_level: WarningLevel,
}

impl BroadcastManager {
    pub fn toggle(&mut self) -> Result<()> {
        if self.enabled {
            self.disable()
        } else {
            self.enable()
        }
    }
    
    pub fn enable(&mut self) -> Result<()> {
        self.detect_ssh_panes();
        
        if self.safety_config.confirm_enable {
            let message = if !self.ssh_panes.is_empty() {
                format!(
                    "Enable broadcast to {} SSH panes?\n\
                     This will type in multiple servers simultaneously.\n\
                     Press Y to confirm, N to cancel.",
                    self.ssh_panes.len()
                )
            } else {
                format!(
                    "Enable broadcast to {} panes?\n\
                     Everything you type will appear in all panes.\n\
                     Press Y to confirm, N to cancel.",
                    self.target_panes.len()
                )
            };
            
            if !self.confirm_dialog(&message)? {
                return Ok(());
            }
        }
        
        if self.safety_config.exclude_local_by_default && !self.ssh_panes.is_empty() {
            self.target_panes = self.ssh_panes.clone();
        } else {
            self.target_panes = self.get_all_panes();
        }
        
        self.enable_visual_indicators()?;
        self.update_status_bar()?;
        self.enabled = true;
        
        self.show_notification(&format!(
            "Broadcasting to {} panes", 
            self.target_panes.len()
        ))?;
        
        Ok(())
    }
    
    pub fn disable(&mut self) -> Result<()> {
        self.enabled = false;
        self.target_panes.clear();
        self.disable_visual_indicators()?;
        self.update_status_bar()?;
        self.show_notification("Broadcast mode disabled")?;
        Ok(())
    }
    
    fn enable_visual_indicators(&self) -> Result<()> {
        let (color, style) = match self.target_panes.len() {
            1..=2 => (Color::Yellow, BorderStyle::Single),
            3..=4 => (Color::Orange, BorderStyle::Double),
            _ => (Color::Red, BorderStyle::Heavy),
        };
        
        for &pane_id in &self.target_panes {
            self.set_pane_border(pane_id, color, style)?;
            self.set_pane_icon(pane_id, "📡")?;
        }
        
        Ok(())
    }
    
    fn detect_ssh_panes(&mut self) {
        self.ssh_panes.clear();
        self.local_panes.clear();
        
        for pane in self.get_all_panes() {
            if let Some(process) = self.get_pane_process(pane) {
                if process.name == "ssh" || 
                   process.name == "mosh" ||
                   process.cmdline.contains("ssh ") {
                    self.ssh_panes.insert(pane);
                } else {
                    self.local_panes.insert(pane);
                }
            }
        }
    }
    
    pub fn broadcast_input(&self, input: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        
        if self.visual_indicators.flash_on_input {
            self.flash_borders()?;
        }
        
        for &pane_id in &self.target_panes {
            self.send_to_pane(pane_id, input)?;
        }
        
        Ok(())
    }
}
```

---

## SEARCH SYSTEM

```rust
pub struct SearchManager {
    current_query: String,
    current_matches: Vec<SearchMatch>,
    current_index: usize,
    
    // Search settings
    case_sensitive: bool,
    use_regex: bool,
    wrap_search: bool,
    incremental: bool,
    
    // Search history
    search_history: VecDeque<String>,
    max_history: usize,
    
    // Search scope
    scope: SearchScope,
}

#[derive(Debug, Clone)]
pub enum SearchScope {
    CurrentPane,
    AllPanes,
    CurrentWindow,
    AllWindows,
    Files(Vec<PathBuf>),
    Commands,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pane_id: PaneId,
    line_number: usize,
    column: usize,
    text: String,
    context_before: String,
    context_after: String,
}

```rust
impl SearchManager {
    pub fn search(&mut self, query: &str) -> Result<Vec<SearchMatch>> {
        self.add_to_history(query.to_string());
        let pattern = self.compile_pattern(query)?;
        
        let matches = match self.scope {
            SearchScope::CurrentPane => {
                self.search_pane(self.current_pane(), &pattern)
            }
            SearchScope::AllPanes => {
                self.search_all_panes(&pattern)
            }
            SearchScope::CurrentWindow => {
                self.search_window(self.current_window(), &pattern)
            }
            SearchScope::AllWindows => {
                self.search_all_windows(&pattern)
            }
            SearchScope::Files(ref paths) => {
                self.search_files(paths, &pattern)
            }
            SearchScope::Commands => {
                self.search_commands(&pattern)
            }
        }?;
        
        self.current_matches = matches.clone();
        self.current_index = 0;
        Ok(matches)
    }
    
    pub fn incremental_search(&mut self, partial: &str) -> Result<Vec<SearchMatch>> {
        if !self.incremental || partial.len() < 2 {
            return Ok(vec![]);
        }
        
        let pattern = self.compile_pattern(partial)?;
        let matches = self.search_limited(&pattern, 50)?;
        Ok(matches)
    }
    
    fn search_pane(&self, pane: &Pane, pattern: &SearchPattern) -> Result<Vec<SearchMatch>> {
        let mut matches = Vec::new();
        let content = pane.get_scrollback();
        
        for (line_num, line) in content.lines().enumerate() {
            if let Some(columns) = pattern.find_in_line(line) {
                for col in columns {
                    matches.push(SearchMatch {
                        pane_id: pane.id,
                        line_number: line_num,
                        column: col,
                        text: line.to_string(),
                        context_before: self.get_context_before(&content, line_num),
                        context_after: self.get_context_after(&content, line_num),
                    });
                }
            }
        }
        
        Ok(matches)
    }
    
    pub fn highlight_matches(&self) -> Result<()> {
        for match_ in &self.current_matches {
            self.highlight_match(match_)?;
        }
        
        if let Some(current) = self.current_matches.get(self.current_index) {
            self.highlight_current_match(current)?;
        }
        
        Ok(())
    }
}
```

---

## PROJECT DETECTION AND TEMPLATES

```rust
pub struct ProjectDetector {
    root: PathBuf,
    project_type: Option<ProjectType>,
    metadata: ProjectMetadata,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Ruby,
    Java,
    DotNet,
    PHP,
    Docker,
    Kubernetes,
    Terraform,
    Ansible,
    Generic,
}

#[derive(Debug, Clone)]
pub struct ProjectMetadata {
    name: String,
    version: Option<String>,
    vcs: Option<VcsType>,
    dependencies: Vec<String>,
    scripts: HashMap<String, String>,
    environments: Vec<String>,
}

impl ProjectDetector {
    pub fn detect(path: &Path) -> ProjectInfo {
        let mut detector = Self {
            root: Self::find_project_root(path),
            project_type: None,
            metadata: ProjectMetadata::default(),
        };
        
        detector.detect_vcs();
        detector.detect_by_manifest();
        detector.detect_by_structure();
        detector.extract_metadata();
        
        ProjectInfo {
            root: detector.root,
            project_type: detector.project_type.unwrap_or(ProjectType::Generic),
            metadata: detector.metadata,
        }
    }
    
    fn find_project_root(path: &Path) -> PathBuf {
        let mut current = path.to_path_buf();
        
        while current.parent().is_some() {
            // VCS roots
            if current.join(".git").exists() ||
               current.join(".hg").exists() ||
               current.join(".svn").exists() {
                return current;
            }
            
            // Project files
            if current.join("Cargo.toml").exists() ||
               current.join("package.json").exists() ||
               current.join("go.mod").exists() ||
               current.join("requirements.txt").exists() ||
               current.join("Gemfile").exists() ||
               current.join("pom.xml").exists() ||
               current.join("docker-compose.yml").exists() {
                return current;
            }
            
            current = current.parent().unwrap().to_path_buf();
        }
        
        path.to_path_buf()
    }
    
    fn detect_by_manifest(&mut self) {
        if self.root.join("Cargo.toml").exists() {
            self.project_type = Some(ProjectType::Rust);
            self.parse_cargo_toml();
        } else if self.root.join("package.json").exists() {
            self.project_type = Some(ProjectType::Node);
            self.parse_package_json();
        } else if self.root.join("requirements.txt").exists() ||
                  self.root.join("setup.py").exists() ||
                  self.root.join("pyproject.toml").exists() {
            self.project_type = Some(ProjectType::Python);
            self.parse_python_files();
        } else if self.root.join("go.mod").exists() {
            self.project_type = Some(ProjectType::Go);
            self.parse_go_mod();
        } else if self.root.join("Gemfile").exists() {
            self.project_type = Some(ProjectType::Ruby);
            self.parse_gemfile();
        } else if self.root.join("pom.xml").exists() {
            self.project_type = Some(ProjectType::Java);
            self.parse_pom_xml();
        } else if self.root.join("*.csproj").exists() {
            self.project_type = Some(ProjectType::DotNet);
            self.parse_dotnet_files();
        } else if self.root.join("docker-compose.yml").exists() {
            self.project_type = Some(ProjectType::Docker);
            self.parse_docker_files();
        }
    }
}

pub struct ProjectTemplates;

impl ProjectTemplates {
    pub fn from_tmux_new(template_type: &str) -> StartupConfig {
        match template_type {
            "ssh" => StartupConfig::Named(HashMap::from([
                ("ssh1".to_string(), "ssh".to_string()),
                ("ssh2".to_string(), "ssh".to_string()),
                ("ssh3".to_string(), "ssh".to_string()),
                ("ssh4".to_string(), "ssh".to_string()),
                ("ssh5".to_string(), "ssh".to_string()),
                ("ssh6".to_string(), "ssh".to_string()),
                ("ssh7".to_string(), "ssh".to_string()),
            ])),
            
            "docker" => StartupConfig::Named(HashMap::from([
                ("shell".to_string(), "".to_string()),
                ("mgr".to_string(), "cd ~/Projects/github/dockermgr".to_string()),
                ("devel".to_string(), "cd ~/Projects/github/casjaysdevdocker".to_string()),
                ("editor".to_string(), "".to_string()),
                ("build".to_string(), "".to_string()),
                ("testing".to_string(), "".to_string()),
                ("docker".to_string(), "".to_string()),
                ("logging".to_string(), "tail -f ~/.local/log/buildx/init.log".to_string()),
                ("remote".to_string(), "ssh".to_string()),
            ])),
            
            "dev" => StartupConfig::Named(HashMap::from([
                ("dev".to_string(), "".to_string()),
                ("editor".to_string(), "".to_string()),
                ("logging".to_string(), "".to_string()),
                ("server".to_string(), "".to_string()),
            ])),
            
            "node" => StartupConfig::Named(HashMap::from([
                ("devel".to_string(), "".to_string()),
                ("editor".to_string(), "".to_string()),
                ("client".to_string(), "".to_string()),
                ("server".to_string(), "npm run dev".to_string()),
                ("database".to_string(), "".to_string()),
            ])),
            
            "build" => StartupConfig::Named(HashMap::from([
                ("build".to_string(), "".to_string()),
                ("edit".to_string(), "".to_string()),
                ("test".to_string(), "cargo test --watch".to_string()),
                ("log".to_string(), "".to_string()),
            ])),
            
            "productivity" => StartupConfig::Named(HashMap::from([
                ("todo".to_string(), "todo".to_string()),
                ("notes".to_string(), "notes".to_string()),
                ("scratchpad".to_string(), "".to_string()),
                ("tasks".to_string(), "task list".to_string()),
                ("blog".to_string(), "".to_string()),
                ("tools".to_string(), "".to_string()),
                ("weather".to_string(), "curl wttr.in".to_string()),
            ])),
            
            _ => StartupConfig::Count(3),
        }
    }
}
```

---

## VCS SUPPORT

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VcsType {
    Git,
    Mercurial,
    Subversion,
    Fossil,
    Bazaar,
    Darcs,
    Pijul,
    CVS,
    Perforce,
    ClearCase,
    TFS,
    Plastic,
    ArX,
    Monotone,
    SCCS,
    RCS,
    BitKeeper,
    Aegis,
    AccuRev,
    SourceSafe,
}

pub struct VcsManager {
    detectors: HashMap<VcsType, Box<dyn VcsDetector>>,
    current_vcs: Option<VcsType>,
    current_status: Option<VcsStatus>,
}

pub trait VcsDetector: Send + Sync {
    fn detect(&self, path: &Path) -> bool;
    fn get_status(&self, path: &Path) -> Result<VcsStatus>;
    fn get_branch(&self, path: &Path) -> Result<String>;
    fn get_remote(&self, path: &Path) -> Result<String>;
}

pub struct VcsStatus {
    branch: String,
    modified: u32,
    untracked: u32,
    conflicts: u32,
    ahead: Option<u32>,
    behind: Option<u32>,
    stashed: Option<u32>,
    shelved: Option<u32>,
    outgoing: Option<u32>,
    incoming: Option<u32>,
    locked: Option<u32>,
}

pub struct GitDetector;

impl VcsDetector for GitDetector {
    fn detect(&self, path: &Path) -> bool {
        self.find_vcs_root(path, ".git").is_some()
    }
    
    fn get_status(&self, path: &Path) -> Result<VcsStatus> {
        let repo = git2::Repository::open(path)?;
        let mut status = VcsStatus::default();
        
        if let Ok(head) = repo.head() {
            if let Some(name) = head.shorthand() {
                status.branch = name.to_string();
            }
        }
        
        let statuses = repo.statuses(None)?;
        for entry in statuses.iter() {
            let flags = entry.status();
            
            if flags.contains(git2::Status::WT_MODIFIED) ||
               flags.contains(git2::Status::INDEX_MODIFIED) {
                status.modified += 1;
            }
            
            if flags.contains(git2::Status::WT_NEW) {
                status.untracked += 1;
            }
            
            if flags.contains(git2::Status::CONFLICTED) {
                status.conflicts += 1;
            }
        }
        
        // Get ahead/behind
        if let Ok((ahead, behind)) = repo.graph_ahead_behind(
            repo.head()?.target().unwrap(),
            repo.find_branch("origin/master", git2::BranchType::Remote)?.get().target().unwrap()
        ) {
            status.ahead = Some(ahead);
            status.behind = Some(behind);
        }
        
        Ok(status)
    }
    
    fn get_branch(&self, path: &Path) -> Result<String> {
        let repo = git2::Repository::open(path)?;
        Ok(repo.head()?
            .shorthand()
            .unwrap_or("HEAD")
            .to_string())
    }
    
    fn get_remote(&self, path: &Path) -> Result<String> {
        let repo = git2::Repository::open(path)?;
        let remote = repo.find_remote("origin")?;
        Ok(remote.url().unwrap_or("").to_string())
    }
}

impl VcsManager {
    pub fn new() -> Self {
        let mut detectors: HashMap<VcsType, Box<dyn VcsDetector>> = HashMap::new();
        
        detectors.insert(VcsType::Git, Box::new(GitDetector));
        detectors.insert(VcsType::Mercurial, Box::new(MercurialDetector));
        detectors.insert(VcsType::Subversion, Box::new(SubversionDetector));
        detectors.insert(VcsType::Fossil, Box::new(FossilDetector));
        detectors.insert(VcsType::Bazaar, Box::new(BazaarDetector));
        detectors.insert(VcsType::Darcs, Box::new(DarcsDetector));
        detectors.insert(VcsType::Pijul, Box::new(PijulDetector));
        detectors.insert(VcsType::CVS, Box::new(CVSDetector));
        detectors.insert(VcsType::Perforce, Box::new(PerforceDetector));
        // ... add all 20 VCS detectors
        
        Self {
            detectors,
            current_vcs: None,
            current_status: None,
        }
    }
    
    pub fn detect(&mut self, path: &Path) -> Option<VcsType> {
        for (&vcs_type, detector) in &self.detectors {
            if detector.detect(path) {
                self.current_vcs = Some(vcs_type);
                return Some(vcs_type);
            }
        }
        None
    }
}
```

---

## AI INTEGRATION

```rust
pub struct AiAssistant {
    ollama_client: OllamaClient,
    model: String,
    context: AiContext,
    history_limit: usize,
}

pub struct OllamaClient {
    base_url: String,
    client: reqwest::Client,
}

pub struct AiContext {
    command_history: Vec<String>,
    current_directory: PathBuf,
    current_files: Vec<String>,
    terminal_output: String,
}

impl AiAssistant {
    pub fn new(config: &AiConfig) -> Self {
        Self {
            ollama_client: OllamaClient::new(&config.ollama_server),
            model: config.model.clone(),
            context: AiContext::default(),
            history_limit: config.history_limit,
        }
    }
    
    pub async fn suggest_command(&self, prompt: &str) -> Result<String> {
        let context = self.build_context_prompt();
        let full_prompt = format!(
            "{}\n\nUser request: {}\n\nSuggest a command:",
            context, prompt
        );
        
        self.ollama_client.generate(&self.model, &full_prompt).await
    }
    
    pub async fn explain_error(&self, error: &str, command: &str) -> Result<String> {
        let prompt = format!(
            "Command: {}\nError: {}\n\nExplain this error and suggest a fix:",
            command, error
        );
        
        self.ollama_client.generate(&self.model, &prompt).await
    }
    
    fn build_context_prompt(&self) -> String {
        format!(
            "Current directory: {}\n\
             Recent commands: {:?}\n\
             Files in directory: {:?}",
            self.context.current_directory.display(),
            self.context.command_history.last_n(5),
            self.context.current_files.first_n(10)
        )
    }
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }
    
    pub async fn generate(&self, model: &str, prompt: &str) -> Result<String> {
        let response = self.client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": model,
                "prompt": prompt,
                "stream": false
            }))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        Ok(response["response"].as_str().unwrap_or("").to_string())
    }
    
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let response = self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        
        let models = response["models"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| m["name"].as_str().map(String::from))
            .collect();
        
        Ok(models)
    }
}
```

---

## SERVICE INTEGRATION

```rust
pub struct ServiceManager {
    pastebin: PastebinService,
    url_shortener: UrlShortenerService,
    custom_services: HashMap<String, CustomService>,
}

pub struct PastebinService {
    provider: String,
    config: HashMap<String, String>,
}

impl PastebinService {
    pub async fn upload(&self, content: &str) -> Result<String> {
        match self.provider.as_str() {
            "dpaste" => self.upload_dpaste(content).await,
            "pastebin" => self.upload_pastebin(content).await,
            "hastebin" => self.upload_hastebin(content).await,
            "ix.io" => self.upload_ix(content).await,
            _ => Err(anyhow!("Unknown pastebin provider: {}", self.provider))
        }
    }
    
    async fn upload_dpaste(&self, content: &str) -> Result<String> {
        let response = reqwest::Client::new()
            .post("https://dpaste.com/api/v2/")
            .form(&[
                ("content", content),
                ("syntax", "text"),
                ("expiry_days", "7"),
            ])
            .send()
            .await?;
        
        Ok(response.headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or_else(|| anyhow!("Failed to get paste URL"))?)
    }
}

pub struct UrlShortenerService {
    provider: String,
}

impl UrlShortenerService {
    pub async fn shorten(&self, url: &str) -> Result<String> {
        match self.provider.as_str() {
            "isgd" => self.shorten_isgd(url).await,
            "tinyurl" => self.shorten_tinyurl(url).await,
            "vgd" => self.shorten_vgd(url).await,
            _ => Err(anyhow!("Unknown URL shortener: {}", self.provider))
        }
    }
    
    async fn shorten_isgd(&self, url: &str) -> Result<String> {
        let response = reqwest::get(format!(
            "https://is.gd/create.php?format=simple&url={}",
            urlencoding::encode(url)
        ))
        .await?
        .text()
        .await?;
        
        Ok(response.trim().to_string())
    }
}
```

---

## FONT SYSTEM

```rust
pub struct FontManager {
    builtin_fonts: HashMap<String, FontData>,
    current_font: FontInfo,
    current_size: f32,
    glyph_cache: GlyphCache,
    allow_system_fonts: bool,
    fallback_chain: Vec<String>,
}

pub struct FontInfo {
    family: String,
    source: FontSource,
    features: FontFeatures,
    metrics: FontMetrics,
}

pub enum FontSource {
    Builtin,
    System(PathBuf),
}

impl FontManager {
    pub fn new() -> Self {
        let mut manager = Self {
            builtin_fonts: HashMap::new(),
            current_font: FontInfo::default(),
            current_size: 14.0,
            glyph_cache: GlyphCache::new(),
            allow_system_fonts: true,
            fallback_chain: vec![],
        };
        
        manager.load_builtin_fonts();
        manager.set_font("Source Code Pro Nerd Font", 14.0).unwrap();
        
        manager
    }
    
    fn load_builtin_fonts(&mut self) {
        // Embed fonts at compile time
        const SOURCE_CODE_PRO: &[u8] = include_bytes!("../assets/fonts/SourceCodePro/SourceCodePro.ttf");
        const JETBRAINS: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono/JetBrainsMono.ttf");
        const HACK: &[u8] = include_bytes!("../assets/fonts/Hack/Hack.ttf");
        const FIRACODE: &[u8] = include_bytes!("../assets/fonts/FiraCode/FiraCode.ttf");
        const IOSEVKA: &[u8] = include_bytes!("../assets/fonts/Iosevka/Iosevka.ttf");
        
        self.builtin_fonts.insert(
            "Source Code Pro Nerd Font".to_string(),
            FontData::from_bytes(SOURCE_CODE_PRO)
        );
        self.builtin_fonts.insert(
            "JetBrains Mono Nerd Font".to_string(),
            FontData::from_bytes(JETBRAINS)
        );
        self.builtin_fonts.insert(
            "Hack Nerd Font".to_string(),
            FontData::from_bytes(HACK)
        );
        self.builtin_fonts.insert(
            "FiraCode Nerd Font".to_string(),
            FontData::from_bytes(FIRACODE)
        );
        self.builtin_fonts.insert(
            "Iosevka Nerd Font".to_string(),
            FontData::from_bytes(IOSEVKA)
        );
    }
    
    pub fn zoom_in(&mut self) {
        self.current_size = (self.current_size + 0.5).min(72.0);
        self.apply_size_change();
    }
    
    pub fn zoom_out(&mut self) {
        self.current_size = (self.current_size - 0.5).max(8.0);
        self.apply_size_change();
    }
    
    pub fn reset_zoom(&mut self) {
        self.current_size = 14.0;
        self.apply_size_change();
    }
    
    fn apply_size_change(&mut self) {
        self.glyph_cache.clear();
        self.update_metrics();
        self.trigger_redraw();
    }
}
```

---

## THEME SYSTEM

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    name: String,
    variant: ThemeVariant,
    colors: ThemeColors,
    ui: ThemeUI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThemeVariant {
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    background: Color,
    foreground: Color,
    cursor: Color,
    selection: Color,
    
    // ANSI colors (0-15)
    black: Color,
    red: Color,
    green: Color,
    yellow: Color,
    blue: Color,
    magenta: Color,
    cyan: Color,
    white: Color,
    
    bright_black: Color,
    bright_red: Color,
    bright_green: Color,
    bright_yellow: Color,
    bright_blue: Color,
    bright_magenta: Color,
    bright_cyan: Color,
    bright_white: Color,
}

pub struct ThemeManager {
    themes: HashMap<String, Theme>,
    current_theme: String,
    auto_detect: bool,
}

impl Theme {
    pub fn dracula() -> Self {
        Self {
            name: "Dracula".to_string(),
            variant: ThemeVariant::Dark,
            colors: ThemeColors {
                background: Color::from_hex("#282a36"),
                foreground: Color::from_hex("#f8f8f2"),
                cursor: Color::from_hex("#f8f8f2"),
                selection: Color::from_hex("#44475a"),
                
                black: Color::from_hex("#21222c"),
                red: Color::from_hex("#ff5555"),
                green: Color::from_hex("#50fa7b"),
                yellow: Color::from_hex("#f1fa8c"),
                blue: Color::from_hex("#bd93f9"),
                magenta: Color::from_hex("#ff79c6"),
                cyan: Color::from_hex("#8be9fd"),
                white: Color::from_hex("#f8f8f2"),
                
                bright_black: Color::from_hex("#6272a4"),
                bright_red: Color::from_hex("#ff6e6e"),
                bright_green: Color::from_hex("#69ff94"),
                bright_yellow: Color::from_hex("#ffffa5"),
                bright_blue: Color::from_hex("#d6acff"),
                bright_magenta: Color::from_hex("#ff92df"),
                bright_cyan: Color::from_hex("#a4ffff"),
                bright_white: Color::from_hex("#ffffff"),
            },
            ui: ThemeUI {
                status_bg: Color::from_hex("#44475a"),
                status_fg: Color::from_hex("#f8f8f2"),
                border_active: Color::from_hex("#bd93f9"),
                border_inactive: Color::from_hex("#44475a"),
                mode_prefix: Color::from_hex("#ff79c6"),
                mode_copy: Color::from_hex("#f1fa8c"),
                mode_broadcast: Color::from_hex("#ff5555"),
            },
        }
    }
}
```

---

## COMMAND PALETTE

```rust
pub struct CommandPalette {
    input: String,
    results: Vec<CommandResult>,
    selected: usize,
    mode: PaletteMode,
    history: VecDeque<String>,
}

#[derive(Debug, Clone)]
pub enum PaletteMode {
    Command,
    GoToPane,
    SearchFiles,
    GoToLine,
    SearchContent,
    Help,
    Fuzzy,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    icon: String,
    title: String,
    description: String,
    action: CommandAction,
    score: f32,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            mode: PaletteMode::Fuzzy,
            history: VecDeque::with_capacity(100),
        }
    }
    
    pub fn update_input(&mut self, input: String) {
        self.input = input;
        self.detect_mode();
        self.update_results();
    }
    
    fn detect_mode(&mut self) {
        self.mode = match self.input.chars().next() {
            Some('>') => PaletteMode::Command,
            Some('@') => PaletteMode::GoToPane,
            Some('#') => PaletteMode::SearchFiles,
            Some(':') => PaletteMode::GoToLine,
            Some('/') => PaletteMode::SearchContent,
            Some('?') => PaletteMode::Help,
            _ => PaletteMode::Fuzzy,
        };
    }
    
    fn update_results(&mut self) {
        let query = match self.mode {
            PaletteMode::Fuzzy => &self.input,
            _ => &self.input[1..],
        };
        
        self.results = match self.mode {
            PaletteMode::Command => self.search_commands(query),
            PaletteMode::GoToPane => self.search_panes(query),
            PaletteMode::SearchFiles => self.search_files(query),
            PaletteMode::GoToLine => self.parse_line_number(query),
            PaletteMode::SearchContent => self.search_content(query),
            PaletteMode::Help => self.search_help(query),
            PaletteMode::Fuzzy => self.search_all(query),
        };
        
        self.results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal)
        });
        
        self.results.truncate(20);
    }
}
```

---

## COMPLETION ENGINE

```rust
pub struct CompletionEngine {
    commands: HashMap<String, CommandCompletion>,
    current_context: CompletionContext,
}

#[derive(Debug, Clone)]
pub struct CommandCompletion {
    name: String,
    subcommands: Vec<String>,
    options: Vec<String>,
    arguments: Vec<ArgumentType>,
}

#[derive(Debug, Clone)]
pub enum ArgumentType {
    File,
    Directory,
    Session,
    Window,
    Pane,
    Command,
    Custom(Vec<String>),
}

impl CompletionEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            commands: HashMap::new(),
            current_context: CompletionContext::default(),
        };
        
        engine.register_builtin_commands();
        engine
    }
    
    fn register_builtin_commands(&mut self) {
        self.register("attach", CommandCompletion {
            name: "attach".to_string(),
            subcommands: vec![],
            options: vec![],
            arguments: vec![ArgumentType::Session],
        });
        
        self.register("list", CommandCompletion {
            name: "list".to_string(),
            subcommands: vec!["sessions", "windows", "panes"],
            options: vec!["--format"],
            arguments: vec![],
        });
        
        self.register("template", CommandCompletion {
            name: "template".to_string(),
            subcommands: vec!["ssh", "docker", "node", "dev", "rpm", "build"],
            options: vec!["--override-window"],
            arguments: vec![],
        });
        
        self.register("cheat", CommandCompletion {
            name: "cheat".to_string(),
            subcommands: vec![],
            options: vec![],
            arguments: vec![ArgumentType::Custom(vec![
                "python", "rust", "go", "javascript", "bash", "git", "docker"
            ])],
        });
        
        self.register("tldr", CommandCompletion {
            name: "tldr".to_string(),
            subcommands: vec![],
            options: vec![],
            arguments: vec![ArgumentType::Command],
        });
        
        self.register("auto_tail", CommandCompletion {
            name: "auto_tail".to_string(),
            subcommands: vec![],
            options: vec!["--dir"],
            arguments: vec![ArgumentType::Directory],
        });
    }
    
    pub fn complete(&self, input: &str, cursor_pos: usize) -> Vec<CompletionItem> {
        let parts: Vec<&str> = input[..cursor_pos].split_whitespace().collect();
        
        if parts.is_empty() {
            return self.complete_commands("");
        }
        
        let cmd = parts[0];
        let current_word = parts.last().unwrap_or("");
        
        if parts.len() == 1 {
            return self.complete_commands(current_word);
        }
        
        if let Some(completion) = self.commands.get(cmd) {
            if parts.len() == 2 && !current_word.starts_with('-') {
                return self.complete_subcommands(completion, current_word);
            }
            
            if current_word.starts_with('-') {
                return self.complete_options(completion, current_word);
            }
            
            return self.complete_arguments(completion, current_word);
        }
        
        Vec::new()
    }
}
```

---

## ERROR HANDLING

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CastermError {
    #[error("Terminal too small: {width}x{height} (minimum {min_width}x{min_height})")]
    TerminalTooSmall {
        width: u16,
        height: u16,
        min_width: u16,
        min_height: u16,
    },
    
    #[error("Configuration error in {file}: {message}")]
    ConfigError {
        file: String,
        message: String,
        line: Option<usize>,
    },
    
    #[error("Failed to connect to {service}: {reason}")]
    ServiceConnectionError {
        service: String,
        reason: String,
    },
    
    #[error("Session '{name}' not found")]
    SessionNotFound { name: String },
    
    #[error("VCS operation failed: {0}")]
    VcsError(String),
    
    #[error("AI service unavailable: {0}")]
    AiServiceError(String),
    
    #[error(transparent)]
    Io(#[from] std::io::Error),
    
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}

pub struct ErrorHandler {
    recovery_strategies: HashMap<ErrorType, RecoveryStrategy>,
}

impl ErrorHandler {
    pub fn handle(&self, error: CastermError) -> Result<()> {
        match error {
            CastermError::AiServiceError(_) => {
                self.disable_ai_features();
                self.notify("AI features unavailable, continuing without them");
                Ok(())
            }
            CastermError::ServiceConnectionError { service, .. } => {
                self.mark_service_unavailable(&service);
                Ok(())
            }
            CastermError::ConfigError { .. } => {
                self.notify("Using default configuration due to config error");
                self.use_default_config();
                Ok(())
            }
            _ => Err(error.into()),
        }
    }
}
```

---

## PERFORMANCE

```rust
pub struct PerformanceMonitor {
    metrics: PerformanceMetrics,
    optimizations: OptimizationFlags,
}

#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    startup_time: Duration,
    average_frame_time: Duration,
    memory_usage: usize,
    cpu_usage: f32,
    render_time: Duration,
    input_processing_time: Duration,
    state_update_time: Duration,
}

pub struct OptimizationFlags {
    gpu_acceleration: bool,
    lazy_rendering: bool,
    dirty_region_tracking: bool,
    scrollback_compression: bool,
    glyph_caching: bool,
    reduce_animations: bool,
}

impl PerformanceMonitor {
    pub fn optimize_for_performance(&mut self) {
        if self.metrics.average_frame_time > Duration::from_millis(20) {
            self.optimizations.reduce_animations = true;
            self.optimizations.lazy_rendering = true;
        }
        
        if self.metrics.memory_usage > 500_000_000 {
            self.trim_scrollback_buffers();
            self.optimizations.scrollback_compression = true;
        }
        
        if self.metrics.cpu_usage > 0.8 {
            self.optimizations.lazy_rendering = true;
        }
    }
}
```

---

## PLATFORM SUPPORT

```rust
pub trait PlatformLayer: Send + Sync {
    fn init() -> Result<()>;
    fn create_window(&self) -> Result<Window>;
    fn get_terminal_info(&self) -> TerminalInfo;
    fn set_clipboard(&self, text: &str) -> Result<()>;
    fn get_clipboard(&self) -> Result<String>;
    fn open_url(&self, url: &str) -> Result<()>;
    fn get_system_info(&self) -> SystemInfo;
}

pub struct LinuxPlatform;

impl PlatformLayer for LinuxPlatform {
    fn set_clipboard(&self, text: &str) -> Result<()> {
        if env::var("WAYLAND_DISPLAY").is_ok() {
            Command::new("wl-copy")
                .stdin(Stdio::piped())
                .spawn()?
                .stdin.unwrap()
                .write_all(text.as_bytes())?;
        } else if env::var("DISPLAY").is_ok() {
            Command::new("xclip")
                .args(&["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()?
                .stdin.unwrap()
                .write_all(text.as_bytes())?;
        } else {
            print!("\x1b]52;c;{}\x07", base64::encode(text));
        }
        Ok(())
    }
}

pub struct MacOSPlatform;

impl PlatformLayer for MacOSPlatform {
    fn set_clipboard(&self, text: &str) -> Result<()> {
        Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()?
            .stdin.unwrap()
            .write_all(text.as_bytes())?;
        Ok(())
    }
}

pub struct WindowsPlatform;

impl PlatformLayer for WindowsPlatform {
    fn set_clipboard(&self, text: &str) -> Result<()> {
        use windows::Win32::System::DataExchange::*;
        
        unsafe {
            OpenClipboard(None)?;
            EmptyClipboard()?;
            
            let len = text.len() + 1;
            let h_mem = GlobalAlloc(GMEM_MOVEABLE, len)?;
            let ptr = GlobalLock(h_mem);
            
            std::ptr::copy_nonoverlapping(
                text.as_ptr(),
                ptr as *mut u8,
                text.len()
            );
            
            GlobalUnlock(h_mem);
            SetClipboardData(CF_TEXT.0, h_mem)?;
            CloseClipboard()?;
        }
        
        Ok(())
    }
}
```

---

## BUILD SYSTEM

### Cargo.toml
```toml
[workspace]
members = ["crates/casterm"]
resolver = "2"

[workspace.package]
version = "1.0.0"
authors = ["CasjaysDev and contributors"]
edition = "2021"
rust-version = "1.70"
license = "MIT"

[workspace.dependencies]
# Terminal Emulation
wezterm-term = "0.1"
termwiz = "0.20"

# GUI/TUI
winit = "0.29"
ratatui = "0.25"
crossterm = "0.27"

# Core
tokio = { version = "1.35", features = ["full"] }
async-trait = "0.1"
futures = "0.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1.0"
bincode = "1.3"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# System
dirs = "5.0"
which = "6.0"
libc = "0.2"
nix = { version = "0.27", features = ["signal", "process"] }

# Networking
reqwest = { version = "0.11", features = ["json", "rustls-tls"] }
url = "2.5"

# VCS
git2 = "0.18"

[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
strip = "symbols"
```

### Makefile
```makefile
# CASTERM Makefile

BINARY_NAME = casterm
VERSION = $(shell grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
TARGET_DIR = target
RELEASE_DIR = $(TARGET_DIR)/release
INSTALL_PREFIX = /usr/local
INSTALL_BIN = $(INSTALL_PREFIX)/bin

# Platform detection
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Linux)
    PLATFORM = linux
endif
ifeq ($(UNAME_S),Darwin)
    PLATFORM = macos
endif
ifeq ($(OS),Windows_NT)
    PLATFORM = windows
    BINARY_NAME := $(BINARY_NAME).exe
endif

ifeq ($(UNAME_M),x86_64)
    ARCH = amd64
endif
ifeq ($(UNAME_M),aarch64)
    ARCH = arm64
endif

.PHONY: all
all: build

.PHONY: build
build:
	cargo build
	@echo "Development build complete: $(TARGET_DIR)/debug/$(BINARY_NAME)"

.PHONY: release
release:
	cargo build --release
	@echo "Release build complete: $(RELEASE_DIR)/$(BINARY_NAME)"

.PHONY: install
install: release
	@echo "Installing $(BINARY_NAME) to $(INSTALL_BIN)"
	@mkdir -p $(INSTALL_BIN)
	@cp $(RELEASE_DIR)/$(BINARY_NAME) $(INSTALL_BIN)/
	@chmod 755 $(INSTALL_BIN)/$(BINARY_NAME)
	@echo "Installation complete"

.PHONY: test
test:
	cargo test --all

.PHONY: clean
clean:
	cargo clean
```

---

## TESTING FRAMEWORK

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_creation() {
        let mut manager = SessionManager::new();
        let session = manager.create_session(Some("test".to_string())).unwrap();
        assert_eq!(manager.sessions.get(&session).unwrap().name, "test");
    }
    
    #[test]
    fn test_vcs_detection() {
        let temp_dir = tempdir::TempDir::new("test").unwrap();
        
        for (marker, expected) in &[
            (".git", VcsType::Git),
            (".hg", VcsType::Mercurial),
            (".svn", VcsType::Subversion),
        ] {
            let vcs_dir = temp_dir.path().join(marker);
            fs::create_dir(&vcs_dir).unwrap();
            
            let mut manager = VcsManager::new();
            assert_eq!(manager.detect(temp_dir.path()), Some(*expected));
            
            fs::remove_dir(&vcs_dir).unwrap();
        }
    }
    
    #[test]
    fn test_builtin_commands() {
        let commands = BuiltinCommands::new();
        
        // Test auto_tail
        assert!(commands.execute("auto_tail", &[]).is_ok());
        
        // Test tldr
        assert!(commands.execute("tldr", &["git".to_string()]).is_ok());
    }
    
    #[test]
    fn test_completion_engine() {
        let engine = CompletionEngine::new();
        
        let completions = engine.complete("att", 3);
        assert!(completions.iter().any(|c| c.value == "attach"));
        
        let completions = engine.complete("list ", 5);
        assert!(completions.iter().any(|c| c.value == "sessions"));
    }
}
```

---

## DISTRIBUTION

### Installation Script
```bash
#!/usr/bin/env bash
# install.sh - Universal CASTERM installer

set -e

VERSION="${1:-latest}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
REPO="https://github.com/casapps/casterm"

detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    
    case "$OS" in
        Linux*)     PLATFORM="linux" ;;
        Darwin*)    PLATFORM="macos" ;;
        MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
        *)          echo "Unsupported OS: $OS"; exit 1 ;;
    esac
    
    case "$ARCH" in
        x86_64|amd64) ARCH="amd64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *)          echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    
    echo "Detected platform: $PLATFORM-$ARCH"
}

download() {
    if [ "$VERSION" = "latest" ]; then
        URL="$REPO/releases/latest/download/casterm-$PLATFORM-$ARCH"
    else
        URL="$REPO/releases/download/v$VERSION/casterm-$PLATFORM-$ARCH"
    fi
    
    echo "Downloading CASTERM from $URL..."
    
    if command -v curl >/dev/null; then
        curl -L -o /tmp/casterm "$URL"
    elif command -v wget >/dev/null; then
        wget -O /tmp/casterm "$URL"
    else
        echo "Error: curl or wget required"
        exit 1
    fi
}

install() {
    echo "Installing to $INSTALL_DIR..."
    
    if [ -w "$INSTALL_DIR" ]; then
        mv /tmp/casterm "$INSTALL_DIR/casterm"
    else
        echo "Root permissions required"
        sudo mv /tmp/casterm "$INSTALL_DIR/casterm"
    fi
    
    chmod +x "$INSTALL_DIR/casterm"
    echo "✓ CASTERM installed successfully!"
}

main() {
    echo "╔════════════════════════════════════════╗"
    echo "║  CASTERM Installer                      ║"
    echo "╚════════════════════════════════════════╝"
    
    detect_platform
    download
    install
    
    echo "Installation complete!"
}

main "$@"
```

---

## IMPLEMENTATION ROADMAP

### Phase 1: Foundation (Month 1-2)
- Terminal emulation with wezterm
- Basic multiplexer (sessions, windows, panes)
- TUI interface with ratatui
- Configuration system
- Status bar implementation

### Phase 2: Core Features (Month 3-4)
- Complete keybinding system
- Copy/paste with clipboard
- Search functionality
- Project detection
- VCS support (all 20 types)
- Built-in commands

### Phase 3: Advanced Features (Month 5-6)
- Broadcast mode
- AI integration
- Service integration
- Command palette
- Completion engine
- All 200+ features

### Phase 4: Polish (Month 7)
- Performance optimization
- Platform-specific fixes
- Theme refinement
- Documentation
- Testing

### Phase 5: Release (Month 8)
- Beta testing
- Package distribution
- Website launch
- Community building

---

## CONCLUSION

CASTERM is a comprehensive terminal emulator with built-in multiplexer that requires zero configuration while providing power users with extensive customization options. By combining the best of modern terminal emulators with tmux-like multiplexing and 200+ built-in features, CASTERM becomes the single terminal solution developers need.

The specification encompasses all aspects from core terminal emulation to advanced features like AI assistance, VCS support for 20+ systems, and intelligent project detection. With its self-contained binary approach and no external dependencies, CASTERM can be deployed anywhere and work immediately.

Total specification: ~5500 lines of comprehensive design and implementation details.

