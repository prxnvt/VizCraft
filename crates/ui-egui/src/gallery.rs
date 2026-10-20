//! The New page: the File page's New screen, Add Page and Help > Examples, over the engine's
//! catalog of document types (`template.list`) and examples (`example.list`), read through
//! [`template::listing`] and [`template::example_listing`] so typing in the search records
//! nothing into a macro.
//!
//! New and Add Page offer **document types**: empty documents set up for a kind of work (the
//! palette's shapes, the notation, the sheet and title block, the rules that check it). Each card
//! shows what the type gives: the empty page or sheet and a row of the shapes its palette offers
//! first. Examples mode offers the filled-in samples with their drawings. The app opens on New
//! when it starts without a file ([`start`]).
//!
//! Top to bottom: the title ("New", "Add Page" or "Examples"), the search field, audience chips,
//! the standards bar (symbol family and sheet, IEC and ISO A3 landscape by default, every other
//! choice offered), the Workspace row (New only, while no workspace preset is chosen), a Recent
//! row (New and Add Page) and one section of cards per discipline (General, Software, Data,
//! Infrastructure, Engineering, then any other a pack names). A selected card opens the details
//! panel on the right, where the viewpoint, the notation variant and the sheet can be chosen
//! before Create, and the rules the type checks with are named.
//!
//! New runs `template.new` with the bar's standards and Examples runs `example.open` (each one
//! undo step back to a blank page; a drawing with unsaved changes first shows a notice with Save
//! First, Replace and Cancel). Add Page runs `template.addPage`, and there the bar edits the
//! document's own standards (`template.standards`). Every choice is the person's; the defaults
//! are only where it starts.
//!
//! `ui.gallery` drives it all by id; `ui.inspect` reports it under `gallery`, with the rects of
//! the chips, cards and buttons for clicks.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Align2, Key, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use vizcraft_diagram::standards::Standards;
use vizcraft_engine::cmd::template::{self, Choices, SHEET_CHOICES, SYMBOL_CHOICES};
use vizcraft_notation::template::{DEFAULT_DISCIPLINE, DISCIPLINES};

use crate::gallery_previews::{self as previews, Drawn};
use crate::theme::{Tokens, medium, regular, semibold};
use crate::{CraftApp, UiState, preset};

/// Most templates the Recent row keeps.
pub const MAX_RECENT: usize = 8;

/// Card size, in points.
const CARD: egui::Vec2 = vec2(220.0, 170.0);
/// Gap between cards.
const CARD_GAP: f32 = 12.0;
/// Height of a card's preview.
const PREVIEW_H: f32 = 100.0;
/// Width of the details panel.
const DETAILS_W: f32 = 320.0;
/// Longest search text kept, in bytes.
const MAX_QUERY: usize = 256;
/// The id prefix of a blank page card for a pack with viewpoints but no templates.
const BLANK: &str = "blank:";

/// What the gallery makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// A new document (File > New).
    #[default]
    New,
    /// A page added to the document (Insert > Pages).
    Page,
    /// A new document from an example (Help > Examples).
    Examples,
}

impl Mode {
    /// The name `ui.gallery` and `ui.inspect` use.
    pub fn name(self) -> &'static str {
        match self {
            Mode::New => "new",
            Mode::Page => "page",
            Mode::Examples => "examples",
        }
    }

    /// Whether the mode lists examples rather than document types.
    pub fn examples(self) -> bool {
        self == Mode::Examples
    }

    /// The page title.
    fn title(self) -> &'static str {
        match self {
            Mode::New => "New",
            Mode::Page => "Add Page",
            Mode::Examples => "Examples",
        }
    }
}

/// One viewpoint a card offers: its id, its label and the labels of the rulesets it checks with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewpointRow {
    pub id: String,
    pub label: String,
    pub rulesets: Vec<String>,
}

/// One card: a row of `template.list` or `example.list`, or a blank page of a pack without
/// document types.
#[derive(Clone, Debug, Default)]
pub struct Card {
    pub id: String,
    pub label: String,
    pub description: String,
    pub pack: String,
    pub pack_name: String,
    /// The New page section it sits in.
    pub discipline: String,
    /// Whether it is an example (opened with `example.open`).
    pub example: bool,
    /// The default viewpoint.
    pub viewpoint: String,
    /// The viewpoints it offers, the default among them.
    pub viewpoints: Vec<ViewpointRow>,
    pub audience: Vec<String>,
    pub keywords: Vec<String>,
    pub code_forms: Vec<String>,
    pub key: bool,
    pub title_block: bool,
    /// The pack's variants: (id, label).
    pub variants: Vec<(String, String)>,
    /// The template's own sheet ("" when it draws none).
    pub sheet: String,
    /// For a blank page card, the viewpoint the page starts on.
    pub blank_viewpoint: Option<String>,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

impl Card {
    fn from_row(r: &Value) -> Card {
        let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let b = |k: &str| r.get(k).and_then(Value::as_bool).unwrap_or(false);
        let variants = r
            .get("variants")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        Some((x.get("id")?.as_str()?.to_string(), x.get("label").and_then(Value::as_str).unwrap_or_default().to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let labels = |v: &Value| -> Vec<String> {
            v.as_array().map(|a| a.iter().filter_map(|r| r.get("label").and_then(Value::as_str)).map(str::to_string).collect()).unwrap_or_default()
        };
        let viewpoints = r
            .get("viewpoints")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        Some(ViewpointRow {
                            id: x.get("id")?.as_str()?.to_string(),
                            label: x.get("label").and_then(Value::as_str).unwrap_or_default().to_string(),
                            rulesets: labels(&x["rulesets"]),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Card {
            id: s("id"),
            label: s("label"),
            description: s("description"),
            pack: s("pack"),
            pack_name: s("packName"),
            discipline: s("discipline"),
            example: false,
            viewpoint: s("viewpoint"),
            viewpoints,
            audience: strings(&r["audience"]),
            keywords: strings(&r["keywords"]),
            code_forms: strings(&r["codeForms"]),
            key: b("key"),
            title_block: b("titleBlock"),
            variants,
            sheet: s("sheet"),
            blank_viewpoint: None,
        }
    }
}

/// A sheet preset offered by the bar: its id and how it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SheetChoice {
    id: String,
    label: String,
}

/// Rects of the last frame, for `ui.inspect` (clicks by agents and tests).
#[derive(Clone, Debug, Default)]
struct Rects {
    chips: Vec<(String, Rect)>,
    cards: Vec<(String, Rect)>,
    recent: Vec<(String, Rect)>,
    buttons: Vec<(String, Rect)>,
}

/// The cards cache: (catalog address, query, audience, examples) and the cards for it.
type Listing = Option<((usize, String, String, bool), Rc<Vec<Card>>)>;

/// The gallery's transient state (on [`CraftApp`], never saved).
#[derive(Default)]
pub struct Gallery {
    /// Showing (on the File page).
    pub open: bool,
    pub mode: Mode,
    /// The search text.
    pub query: String,
    /// The audience chip ("" is All).
    pub audience: String,
    /// The selected card's id.
    pub selected: Option<String>,
    /// The viewpoint the person chose in the details panel (`None`: the type's default).
    pub viewpoint: Option<String>,
    /// The Notation the person chose in the details panel (`None`: what the engine would choose;
    /// "" is the default drawing).
    pub variant: Option<String>,
    /// The sheet chosen in the details panel (`None`: the project's; "" is no sheet).
    pub sheet: Option<String>,
    /// The Workspace row's choice, when the person changed it from the one the audience suggests.
    pub workspace: Option<&'static str>,
    /// The card whose Create waits on the unsaved-changes notice.
    pub notice: Option<String>,
    /// Previews still waiting to be built, as of the last frame.
    pub pending: usize,
    /// Templates whose preview failed to build ("No preview"), as of the last frame.
    pub failed: Vec<String>,
    /// Preview textures, as of the last frame.
    pub textures: usize,
    focus: bool,
    cols: usize,
    presets: Vec<SheetChoice>,
    listing: RefCell<Listing>,
    rects: Rects,
}

/// The status bar text and the `ui.gallery` error for a create that would replace unsaved changes.
pub const NOTICE: &str = "The drawing has unsaved changes; creating replaces it";

impl Gallery {
    /// The cards shown for the mode, the search and the audience, grouped by discipline and in
    /// catalog order inside each, the blank page cards of packs without document types after
    /// their pack's place.
    pub fn cards(&self) -> Rc<Vec<Card>> {
        let key = (previews::catalog_address(), self.query.trim().to_string(), self.audience.clone(), self.mode.examples());
        if let Some((k, cards)) = self.listing.borrow().as_ref()
            && *k == key
        {
            return cards.clone();
        }
        let audience = (!key.2.is_empty()).then_some(key.2.as_str());
        let query = (!key.1.is_empty()).then_some(key.1.as_str());
        let cards = Rc::new(cards_for(key.3, audience, query));
        *self.listing.borrow_mut() = Some((key, cards.clone()));
        cards
    }

    fn shown_ids(&self) -> Vec<String> {
        self.cards().iter().map(|c| c.id.clone()).collect()
    }

    /// Forget the per-template choices (a new selection starts from the defaults).
    fn reset_choices(&mut self) {
        self.viewpoint = None;
        self.variant = None;
        self.sheet = None;
        self.notice = None;
    }
}

/// Where discipline `d` sits on the New page: the known ones in [`DISCIPLINES`] order, any other
/// after them.
fn discipline_rank(d: &str) -> usize {
    DISCIPLINES.iter().position(|x| *x == d).unwrap_or(DISCIPLINES.len())
}

/// The heading of discipline `d`.
pub fn discipline_label(d: &str) -> String {
    title_case(if d.is_empty() { DEFAULT_DISCIPLINE } else { d })
}

/// The cards for these filters: the examples (`template::example_listing`), or the document
/// types (`template::listing`) plus a blank page card for each pack that has viewpoints and no
/// document types, matched the same way. Grouped by discipline, catalog order inside each.
fn cards_for(examples: bool, audience: Option<&str>, query: Option<&str>) -> Vec<Card> {
    let mut out = if examples { examples_for(audience, query) } else { types_for(audience, query) };
    out.sort_by(|a, b| discipline_rank(&a.discipline).cmp(&discipline_rank(&b.discipline)).then_with(|| a.discipline.cmp(&b.discipline)));
    out
}

fn examples_for(audience: Option<&str>, query: Option<&str>) -> Vec<Card> {
    template::example_listing(audience, None, query).iter().map(|r| Card { example: true, ..Card::from_row(r) }).collect()
}

fn types_for(audience: Option<&str>, query: Option<&str>) -> Vec<Card> {
    let rows = template::listing(audience, None, query);
    let mut out = Vec::new();
    for pack in vizcraft_notation::catalog().packs() {
        out.extend(rows.iter().filter(|r| r["pack"] == pack.id.as_str()).map(Card::from_row));
        if pack.templates.is_empty()
            && let Some(vp) = pack.viewpoints.first()
        {
            let label = format!("Blank {} page", pack.name);
            let audience_ok = audience.is_none_or(|a| pack.audience.iter().any(|x| x.eq_ignore_ascii_case(a)));
            let query_ok = query.is_none_or(|q| {
                let q = q.to_lowercase();
                label.to_lowercase().contains(&q) || pack.id.to_lowercase().contains(&q) || pack.description.to_lowercase().contains(&q)
            });
            if audience_ok && query_ok {
                out.push(Card {
                    id: format!("{BLANK}{}", pack.id),
                    label,
                    description: pack.description.clone(),
                    pack: pack.id.clone(),
                    pack_name: pack.name.clone(),
                    discipline: DEFAULT_DISCIPLINE.into(),
                    audience: pack.audience.clone(),
                    blank_viewpoint: Some(vp.id.clone()),
                    ..Card::default()
                });
            }
        }
    }
    out
}

/// Every audience of the mode's cards, in the order first seen.
fn audiences(examples: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in cards_for(examples, None, None) {
        for a in c.audience {
            if !out.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
                out.push(a);
            }
        }
    }
    out
}

/// A word with its first letter capitalised.
fn title_case(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// What a symbol family stands for, for its tooltip.
fn symbols_tip(family: &str) -> &'static str {
    match family {
        "iec" => "IEC 60617 graphical symbols (the international default)",
        "ansi" => "ANSI symbols, drawn as IEEE 315 describes them",
        "jic" => "JIC symbols for industrial control panels",
        "ieee" => "IEEE 315 graphic symbols",
        "isa" => "ISA 5.1 instrumentation symbols",
        "iso" => "ISO symbols (ISO 14617, ISO 1219 for fluid power)",
        _ => "A symbol family",
    }
}

/// The sheet presets `sheet.presets` offers, each labelled "<standard> <size> <orientation>",
/// ISO first; the template module's short list when the command is not registered.
fn read_presets(app: &mut CraftApp) -> Vec<SheetChoice> {
    let reply = app.session.run("sheet.presets", &json!({})).unwrap_or(Value::Null);
    let mut out = Vec::new();
    for st in reply["standards"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let sid = st["id"].as_str().unwrap_or_default();
        let label = st["label"].as_str().unwrap_or(sid);
        for orientation in ["landscape", "portrait"] {
            for sz in st["sizes"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let size = sz["id"].as_str().unwrap_or_default();
                let name = sz["name"].as_str().unwrap_or(size);
                out.push(SheetChoice { id: format!("{sid}.{size}.{orientation}"), label: format!("{label} {name} {orientation}") });
            }
        }
    }
    if out.is_empty() {
        out = SHEET_CHOICES.iter().map(|id| SheetChoice { id: id.to_string(), label: id.to_string() }).collect();
    }
    // ISO first, as `sheet.presets` lists it; a stable sort keeps the size order inside each.
    out.sort_by_key(|c| !c.id.starts_with("iso."));
    out
}

/// A sheet id as `template.standards` stores it: `ansi.` read as `asme.`, the orientation added.
fn full_sheet(sheet: &str) -> String {
    let s = sheet.trim().to_ascii_lowercase();
    let s = match s.strip_prefix("ansi.") {
        Some(rest) => format!("asme.{rest}"),
        None => s,
    };
    if s.split('.').count() == 2 { format!("{s}.landscape") } else { s }
}

/// The standards the bar shows: in New and Examples the last choice (the preferences), in Add
/// Page the document's; an unset field reads as its default.
pub fn bar(app: &CraftApp) -> Standards {
    let st = match app.gallery.mode {
        Mode::New | Mode::Examples => &app.ui.new_standards,
        Mode::Page => &app.session.doc.standards,
    };
    Standards { symbols: st.symbols().to_string(), sheet: st.sheet().to_string() }
}

/// Put template `id` at the front of the Recent row (kept to [`MAX_RECENT`], no repeats).
pub fn push_recent(ui: &mut UiState, id: &str) {
    if id.is_empty() || id.len() > vizcraft_notation::template::MAX_ID {
        return;
    }
    ui.recent_templates.retain(|r| r != id);
    ui.recent_templates.insert(0, id.to_string());
    ui.recent_templates.truncate(MAX_RECENT);
}

/// Clean the gallery's preferences after loading them: a known preset or none, a short Recent
/// list without repeats, and standards cut to size.
pub fn sanitize_prefs(ui: &mut UiState) {
    if !preset::PRESETS.contains(&ui.preset.as_str()) {
        ui.preset.clear();
    }
    let mut seen: Vec<String> = Vec::new();
    for id in std::mem::take(&mut ui.recent_templates) {
        if !id.is_empty() && id.len() <= vizcraft_notation::template::MAX_ID && !seen.contains(&id) && seen.len() < MAX_RECENT {
            seen.push(id);
        }
    }
    ui.recent_templates = seen;
    ui.new_standards.sanitize();
    if !ui.new_standards.symbols.is_empty() && !SYMBOL_CHOICES.contains(&ui.new_standards.symbols.as_str()) {
        ui.new_standards.symbols.clear();
    }
    if !ui.new_standards.sheet.is_empty() && !vizcraft_notation::template::is_sheet_id(&ui.new_standards.sheet) {
        ui.new_standards.sheet.clear();
    }
}

/// Open the New page at launch: the gallery in New mode on the File page. The app calls this when
/// it starts without a file to open; Escape or Blank Drawing gives the blank page, and a file
/// opened afterwards (Finder, File > Open) closes it.
pub fn start(app: &mut CraftApp) {
    open(app, Mode::New);
}

/// Open the gallery in `mode` on the File page, fresh: no search, All, nothing selected.
pub fn open(app: &mut CraftApp, mode: Mode) {
    let presets = read_presets(app);
    let g = &mut app.gallery;
    g.open = true;
    g.mode = mode;
    g.query.clear();
    g.audience.clear();
    g.selected = None;
    g.workspace = None;
    g.reset_choices();
    g.focus = true;
    g.presets = presets;
    app.backstage = true;
}

/// Close the gallery and the File page.
pub fn close(app: &mut CraftApp) {
    app.gallery.open = false;
    app.gallery.notice = None;
    let _ = app.run("ui.backstage", json!({"value": false}));
}

/// Escape: clear a non-empty search first, then close the notice, then the gallery and the File page.
pub fn escape(app: &mut CraftApp) {
    if !app.gallery.query.is_empty() {
        app.gallery.query.clear();
    } else if app.gallery.notice.is_some() {
        app.gallery.notice = None;
    } else {
        close(app);
    }
}

/// Set the bar's symbol family and sheet: in New and Examples they become the next default
/// (preferences); in Add Page they are the document's standards, one undo step each.
fn set_standards(app: &mut CraftApp, symbols: Option<&str>, sheet: Option<&str>) -> Result<(), String> {
    match app.gallery.mode {
        Mode::New | Mode::Examples => {
            if let Some(s) = symbols {
                app.ui.new_standards.symbols = s.to_string();
            }
            if let Some(s) = sheet {
                app.ui.new_standards.sheet = s.to_string();
            }
            Ok(())
        }
        Mode::Page => {
            for (k, v) in [("symbols", symbols), ("sheet", sheet)] {
                if let Some(v) = v {
                    app.run("template.standards", json!({ k: v }))?;
                }
            }
            Ok(())
        }
    }
}

/// The workspace the Workspace row stands on: the person's pick, else the audience's suggestion.
fn workspace_choice(g: &Gallery) -> &'static str {
    g.workspace.unwrap_or_else(|| preset::suggested(&g.audience))
}

/// The card `id` of the mode: among the cards shown, else any of the mode (a Recent card).
fn find_card(app: &CraftApp, id: &str) -> Option<Card> {
    let cards = app.gallery.cards();
    cards.iter().find(|c| c.id == id).cloned().or_else(|| cards_for(app.gallery.mode.examples(), None, None).into_iter().find(|c| c.id == id))
}

/// Create from card `id` in the gallery's mode. In New and Examples a drawing with unsaved
/// changes is not replaced unless `replace`: the notice shows and the result is an error naming
/// it. On success a document type goes to Recent (in `CraftApp::run`), a pending workspace
/// choice is applied, and the File page closes; on failure the engine's message is in the
/// status bar and the gallery stays open.
pub fn create(app: &mut CraftApp, id: &str, replace: bool) -> Result<Value, String> {
    let Some(card) = find_card(app, id) else {
        return Err(format!("unknown document type `{}`; ui.gallery lists them under shown", id.chars().take(64).collect::<String>()));
    };
    let mode = app.gallery.mode;
    if mode != Mode::Page && app.session.dirty && !replace {
        app.gallery.selected = Some(card.id.clone());
        app.gallery.notice = Some(card.id.clone());
        return Err(format!("{NOTICE} (Save First, Replace or Cancel)"));
    }
    let r = match (&card.blank_viewpoint, mode) {
        (Some(vp), Mode::New | Mode::Examples) => {
            app.run("file.new", json!({})).and_then(|_| app.run("model.setViewpoint", json!({"viewpoint": vp})))
        }
        (Some(vp), Mode::Page) => app.run(
            "edit.batch",
            json!({"label": format!("New {} Page", card.pack_name), "commands": [
                {"command": "page.add"},
                {"command": "model.setViewpoint", "params": {"viewpoint": vp}},
            ]}),
        ),
        (None, _) => {
            let mut p = if mode.examples() { json!({"example": card.id}) } else { json!({"template": card.id}) };
            if let Some(v) = &app.gallery.viewpoint {
                p["viewpoint"] = json!(v);
            }
            if let Some(v) = &app.gallery.variant {
                p["variant"] = json!(v);
            }
            if let Some(s) = &app.gallery.sheet {
                p["sheet"] = json!(s);
            }
            match mode {
                Mode::Page => app.run("template.addPage", p),
                Mode::New | Mode::Examples => {
                    let st = bar(app);
                    p["standards"] = json!({"symbols": st.symbols, "sheet": st.sheet});
                    app.run(if mode.examples() { "example.open" } else { "template.new" }, p)
                }
            }
        }
    };
    let out = r?;
    app.gallery.notice = None;
    let workspace = (mode == Mode::New && app.ui.preset.is_empty()).then(|| workspace_choice(&app.gallery));
    app.gallery.open = false;
    let _ = app.run("ui.backstage", json!({"value": false}));
    if mode != Mode::Page {
        let _ = app.run("ui.pane.shapeData", json!({"value": true}));
    }
    if let Some(name) = workspace
        && app.run("ui.preset", json!({"name": name})).is_ok()
    {
        app.status(if name == preset::ENGINEERING {
            "Engineering workspace on; View > Workspace switches back"
        } else {
            "Office workspace on; View > Workspace switches"
        });
    }
    Ok(out)
}

/// Save, then create card `id` when the save went through (a cancelled Save As keeps the notice).
fn save_first(app: &mut CraftApp, id: &str) {
    let _ = app.run("file.save", json!({}));
    if !app.session.dirty {
        let _ = create(app, id, true);
    }
}

/// The `ui.gallery` result: the state an agent needs to drive it.
fn state(app: &CraftApp) -> Value {
    let g = &app.gallery;
    let st = bar(app);
    json!({
        "open": g.open && app.backstage,
        "mode": g.mode.name(),
        "query": g.query,
        "audience": g.audience,
        "selected": g.selected,
        "viewpoint": g.viewpoint,
        "shown": g.shown_ids(),
        "pending": g.pending,
        "symbols": st.symbols,
        "sheet": st.sheet,
    })
}

/// What `ui.inspect` reports under `gallery`: the `ui.gallery` state plus the notice, the
/// workspace choice, Recent, the texture count and the rects of chips, cards and buttons.
pub fn inspect(app: &CraftApp) -> Value {
    let g = &app.gallery;
    let rect = |r: &Rect| json!([r.min.x, r.min.y, r.max.x, r.max.y]);
    let rows = |v: &[(String, Rect)], k: &str| v.iter().map(|(id, r)| json!({ k: id, "rect": rect(r) })).collect::<Vec<_>>();
    let mut out = state(app);
    if let Some(o) = out.as_object_mut() {
        o.insert("notice".into(), json!(g.notice));
        o.insert("workspace".into(), json!(app.ui.preset.is_empty().then(|| workspace_choice(g))));
        o.insert("recent".into(), json!(app.ui.recent_templates));
        o.insert("textures".into(), json!(g.textures));
        o.insert("failed".into(), json!(g.failed));
        o.insert("chips".into(), json!(rows(&g.rects.chips, "label")));
        o.insert("cards".into(), json!(rows(&g.rects.cards, "id")));
        o.insert("recentCards".into(), json!(rows(&g.rects.recent, "id")));
        o.insert("buttons".into(), json!(rows(&g.rects.buttons, "label")));
    }
    out
}

/// `ui.gallery`, and the closing of the gallery whenever the File page is left another way.
pub fn ui_command(app: &mut CraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    match id {
        "ui.backstage" | "ui.tab" | "ui.present" => {
            // Leaving or reopening the File page another way shows its usual page next time.
            app.gallery.open = false;
            app.gallery.notice = None;
            None
        }
        "ui.gallery" => Some(command(app, p)),
        _ => None,
    }
}

fn command(app: &mut CraftApp, p: &Value) -> Result<Value, String> {
    let text = |k: &str| -> Result<Option<&str>, String> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.as_str())),
            Some(_) => Err(format!("`{k}` must be a string")),
        }
    };
    let flag = |k: &str| p.get(k).and_then(Value::as_bool);
    // Everything is checked before anything changes.
    let mode = match text("mode")? {
        None => None,
        Some("new") => Some(Mode::New),
        Some("page") => Some(Mode::Page),
        Some("examples") => Some(Mode::Examples),
        Some(m) => return Err(format!("`mode` is new, page or examples, not `{}`", m.chars().take(64).collect::<String>())),
    };
    // The mode the gallery will be in: the one given, else the open one's, else New.
    let target = mode.unwrap_or(if app.gallery.open && app.backstage { app.gallery.mode } else { Mode::New });
    let all = cards_for(target.examples(), None, None);
    let select = text("select")?;
    if let Some(id) = select
        && !all.iter().any(|c| c.id == id)
    {
        let list = if target.examples() { "example.list" } else { "template.list" };
        return Err(format!(
            "unknown {} `{}`; {list} lists them",
            if target.examples() { "example" } else { "document type" },
            id.chars().take(64).collect::<String>()
        ));
    }
    let viewpoint = text("viewpoint")?;
    if let Some(v) = viewpoint {
        let card =
            select.map(str::to_string).or_else(|| (mode.is_none_or(|m| m == app.gallery.mode)).then(|| app.gallery.selected.clone()).flatten());
        let offered = card.and_then(|id| all.iter().find(|c| c.id == id).map(|c| c.viewpoints.iter().any(|x| x.id == v)));
        match offered {
            Some(true) => {}
            Some(false) => return Err(format!("`{}` is not a viewpoint the selected card offers", v.chars().take(64).collect::<String>())),
            None => return Err("`viewpoint` needs a selected card (`select`)".into()),
        }
    }
    let symbols = text("symbols")?.map(str::to_ascii_lowercase);
    if let Some(s) = &symbols
        && !SYMBOL_CHOICES.contains(&s.as_str())
    {
        return Err(format!("`symbols` is one of {}", SYMBOL_CHOICES.join(", ")));
    }
    let sheet = text("sheet")?.map(full_sheet);
    let query = text("query")?;
    let audience = text("audience")?;
    let presets = if app.gallery.presets.is_empty() || sheet.is_some() { read_presets(app) } else { app.gallery.presets.clone() };
    if let Some(s) = &sheet
        && !presets.iter().any(|c| c.id == *s)
    {
        return Err(format!("`{}` is not a sheet preset; sheet.presets lists them", s.chars().take(64).collect::<String>()));
    }

    let want = flag("open").unwrap_or(true);
    if want && !(app.gallery.open && app.backstage) {
        open(app, mode.unwrap_or_default());
    } else if !want {
        close(app);
    }
    if let Some(m) = mode
        && m != app.gallery.mode
    {
        app.gallery.mode = m;
        app.gallery.selected = None;
        app.gallery.reset_choices();
    }
    if let Some(q) = query {
        let mut end = q.len().min(MAX_QUERY);
        while !q.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        app.gallery.query = q.get(..end).unwrap_or_default().to_string();
    }
    if let Some(a) = audience {
        app.gallery.audience = a.chars().take(64).collect();
    }
    if let Some(id) = select {
        if app.gallery.selected.as_deref() != Some(id) {
            app.gallery.reset_choices();
        }
        app.gallery.selected = Some(id.to_string());
    }
    if let Some(v) = viewpoint {
        app.gallery.viewpoint = Some(v.to_string());
    }
    if symbols.is_some() || sheet.is_some() {
        set_standards(app, symbols.as_deref(), sheet.as_deref())?;
    }
    if flag("clearRecent") == Some(true) {
        app.ui.recent_templates.clear();
    }
    if flag("create") == Some(true) {
        let id = app.gallery.selected.clone().or_else(|| app.gallery.shown_ids().into_iter().next());
        let Some(id) = id else { return Err("no card is shown to create".into()) };
        create(app, &id, false)?;
    }
    Ok(state(app))
}

/// Keyboard: arrows move the selection (Up and Down by row; Left and Right by card while the
/// search is empty), Enter creates the selected card or the first one shown. Consumed before
/// the search field is drawn, so it does not see them.
fn keys(app: &mut CraftApp, ctx: &egui::Context) {
    if app.dialog.is_some() {
        return;
    }
    let ids = app.gallery.shown_ids();
    if ids.is_empty() {
        return;
    }
    let cols = app.gallery.cols.max(1) as isize;
    let empty = app.gallery.query.is_empty();
    let step = ctx.input_mut(|i| {
        if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
            Some(cols)
        } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
            Some(-cols)
        } else if empty && i.consume_key(Modifiers::NONE, Key::ArrowRight) {
            Some(1)
        } else if empty && i.consume_key(Modifiers::NONE, Key::ArrowLeft) {
            Some(-1)
        } else {
            None
        }
    });
    if let Some(step) = step {
        let at = app.gallery.selected.as_ref().and_then(|s| ids.iter().position(|i| i == s));
        let next = match at {
            Some(i) => (i as isize + step).clamp(0, ids.len() as isize - 1) as usize,
            None => 0,
        };
        if let Some(id) = ids.get(next)
            && app.gallery.selected.as_deref() != Some(id.as_str())
        {
            app.gallery.selected = Some(id.clone());
            app.gallery.reset_choices();
        }
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
        let id = app.gallery.selected.clone().or_else(|| ids.first().cloned());
        if let Some(id) = id {
            let _ = create(app, &id, false);
        }
    }
}

/// Draw the gallery (inside the File page's central area).
pub fn show(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    keys(app, &ctx);
    if !app.gallery.open {
        return;
    }
    let mut rects = Rects::default();
    let mode = app.gallery.mode;
    ui.label(egui::RichText::new(mode.title()).font(semibold(24.0)).color(t.text));
    ui.add_space(10.0);

    // Search. Typing anywhere on the page while no other text field has the keyboard goes here.
    let search_id = egui::Id::new("gallery_search");
    let typed = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))));
    let elsewhere = ctx.memory(|m| m.focused()).is_some_and(|f| f != search_id && egui::text_edit::TextEditState::load(&ctx, f).is_some());
    if app.gallery.focus || (typed && !elsewhere) {
        ctx.memory_mut(|m| m.request_focus(search_id));
        app.gallery.focus = false;
    }
    let mut query = app.gallery.query.clone();
    let hint = if mode.examples() { "Search examples" } else { "Search document types" };
    let resp = ui.add(egui::TextEdit::singleline(&mut query).id(search_id).hint_text(hint).desired_width(360.0).font(regular(13.0)));
    resp.widget_info(|| egui::WidgetInfo::text_edit(true, "", query.as_str(), hint));
    let resp = resp.on_hover_text(hint);
    if resp.changed() {
        let mut end = query.len().min(MAX_QUERY);
        while !query.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        app.gallery.query = query.get(..end).unwrap_or_default().to_string();
    }
    ui.add_space(8.0);

    // Audience chips.
    ui.horizontal_wrapped(|ui| {
        let mut chips = vec![(String::new(), "All".to_string())];
        chips.extend(audiences(mode.examples()).into_iter().map(|a| (a.clone(), title_case(&a))));
        for (value, label) in chips {
            let on = app.gallery.audience.eq_ignore_ascii_case(&value);
            let r = ui.add(egui::Button::selectable(on, egui::RichText::new(&label).font(regular(12.5))));
            rects.chips.push((label, r.rect));
            if r.clicked() {
                app.gallery.audience = value;
            }
        }
    });
    ui.add_space(6.0);
    standards_bar(app, ui, &mut rects);
    if app.gallery.mode == Mode::New && app.ui.preset.is_empty() {
        ui.add_space(4.0);
        workspace_row(app, ui);
    }
    ui.add_space(10.0);

    // The cards on the left, the details on the right while a card is selected.
    let area = ui.available_rect_before_wrap();
    let cards = app.gallery.cards();
    if app.gallery.selected.as_ref().is_some_and(|s| !cards.iter().any(|c| c.id == *s) && (mode.examples() || !app.ui.recent_templates.contains(s))) {
        app.gallery.selected = None;
    }
    let selected = app.gallery.selected.clone().and_then(|s| find_card(app, &s));
    let list_rect =
        if selected.is_some() { Rect::from_min_max(area.min, pos2((area.max.x - DETAILS_W - 16.0).max(area.min.x), area.max.y)) } else { area };
    let mut tally = Tally::default();
    let mut chosen: Option<(String, bool)> = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(list_rect), |ui| {
        egui::ScrollArea::vertical().id_salt("gallery_cards").auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width();
            app.gallery.cols = (((width + CARD_GAP) / (CARD.x + CARD_GAP)).floor() as usize).max(1);
            let recent: Vec<Card> = if app.gallery.query.is_empty() && !mode.examples() {
                let all = cards_for(false, None, None);
                app.ui.recent_templates.iter().filter_map(|id| all.iter().find(|c| c.id == *id).cloned()).collect()
            } else {
                Vec::new()
            };
            if !recent.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Recent").font(medium(14.0)).color(t.text));
                    let r = ui.add(egui::Button::new(egui::RichText::new("Clear").font(regular(11.5)).color(t.accent_text)).frame(false));
                    rects.buttons.push(("Clear".into(), r.rect));
                    if r.clicked() {
                        app.ui.recent_templates.clear();
                    }
                });
                ui.add_space(4.0);
                grid(ui, |ui| {
                    for c in &recent {
                        let (r, got) = card(app, ui, c, true, &mut tally);
                        rects.recent.push((c.id.clone(), r));
                        if got.is_some() {
                            chosen = got.map(|d| (c.id.clone(), d));
                        }
                    }
                });
                ui.add_space(14.0);
            }
            let mut i = 0;
            while i < cards.len() {
                let discipline = cards.get(i).map(|c| c.discipline.clone()).unwrap_or_default();
                let end = cards.iter().skip(i).position(|c| c.discipline != discipline).map_or(cards.len(), |n| i + n);
                let group = cards.get(i..end).unwrap_or_default();
                if !group.is_empty() {
                    ui.label(egui::RichText::new(discipline_label(&discipline)).font(medium(14.0)).color(t.text));
                    ui.add_space(4.0);
                }
                grid(ui, |ui| {
                    for c in group {
                        let (r, got) = card(app, ui, c, false, &mut tally);
                        rects.cards.push((c.id.clone(), r));
                        if got.is_some() {
                            chosen = got.map(|d| (c.id.clone(), d));
                        }
                    }
                });
                ui.add_space(14.0);
                i = end.max(i + 1);
            }
            if cards.is_empty() {
                let none = if mode.examples() { "No examples match" } else { "No document types match" };
                ui.label(egui::RichText::new(none).color(t.text_dim));
            }
        });
    });
    if let Some(c) = &selected {
        let r = Rect::from_min_max(pos2(area.max.x - DETAILS_W, area.min.y), area.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
            details(app, ui, c, &mut rects, &mut tally);
        });
    }
    ui.allocate_rect(area, Sense::hover());
    if let Some((id, double)) = chosen {
        if app.gallery.selected.as_deref() != Some(id.as_str()) {
            app.gallery.reset_choices();
        }
        app.gallery.selected = Some(id.clone());
        if double {
            let _ = create(app, &id, false);
        }
    }
    app.gallery.pending = tally.pending;
    app.gallery.failed = tally.failed;
    app.gallery.textures = previews::textures(&ctx);
    app.gallery.rects = rects;
}

/// A wrapping row of cards.
fn grid(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(CARD_GAP, CARD_GAP);
        add(ui);
    });
}

/// The standards bar: Symbols (IEC first, every family offered) and Sheet (every preset of the
/// orientation chosen, ISO first, then ASME) with Landscape and Portrait.
fn standards_bar(app: &mut CraftApp, ui: &mut Ui, rects: &mut Rects) {
    let st = bar(app);
    let mut symbols: Option<&str> = None;
    let mut sheet: Option<String> = None;
    ui.horizontal(|ui| {
        egui::ComboBox::new("gallery_symbols", "Symbols").selected_text(st.symbols.to_uppercase()).width(90.0).show_ui(ui, |ui| {
            for s in SYMBOL_CHOICES {
                if ui.selectable_label(st.symbols == *s, s.to_uppercase()).on_hover_text(symbols_tip(s)).clicked() {
                    symbols = Some(s);
                }
            }
        });
        ui.add_space(16.0);
        let orientation = st.sheet.rsplit('.').next().unwrap_or("landscape").to_string();
        let label = app.gallery.presets.iter().find(|c| c.id == st.sheet).map_or(st.sheet.clone(), |c| c.label.clone());
        egui::ComboBox::new("gallery_sheet", "Sheet size").selected_text(label).width(230.0).show_ui(ui, |ui| {
            for c in app.gallery.presets.iter().filter(|c| c.id.ends_with(&format!(".{orientation}"))) {
                if ui.selectable_label(c.id == st.sheet, &c.label).clicked() {
                    sheet = Some(c.id.clone());
                }
            }
        });
        ui.add_space(8.0);
        for o in ["landscape", "portrait"] {
            let r = ui.add(egui::Button::selectable(orientation == o, title_case(o)));
            rects.buttons.push((title_case(o), r.rect));
            if r.clicked() && orientation != o {
                let base = st.sheet.rsplit_once('.').map_or(st.sheet.as_str(), |(b, _)| b);
                let id = format!("{base}.{o}");
                if app.gallery.presets.iter().any(|c| c.id == id) {
                    sheet = Some(id);
                }
            }
        }
        let what = if app.gallery.mode == Mode::Page { "The document's standards" } else { "Standards for the new drawing" };
        ui.label(egui::RichText::new(what).font(regular(11.0)).color(Tokens::get(ui.ctx()).text_dim));
    });
    if (symbols.is_some() || sheet.is_some())
        && let Err(e) = set_standards(app, symbols, sheet.as_deref())
    {
        app.status(e);
    }
}

/// The Workspace row: Engineering or Office, applied with the first Create.
fn workspace_row(app: &mut CraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let current = workspace_choice(&app.gallery);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Workspace").font(regular(12.5)).color(t.text));
        for (name, label) in [(preset::ENGINEERING, "Engineering workspace"), (preset::OFFICE, "Office workspace")] {
            if ui.add(egui::Button::selectable(current == name, label)).clicked() {
                app.gallery.workspace = Some(name);
            }
        }
        ui.label(egui::RichText::new("Applied when you create; View > Workspace changes it later").font(regular(11.0)).color(t.text_dim));
    });
}

/// Previews drawn this frame that wait for a build, and templates whose build failed.
#[derive(Default)]
struct Tally {
    pending: usize,
    failed: Vec<String>,
}

impl Tally {
    fn count(&mut self, id: &str, d: Drawn) {
        match d {
            Drawn::Pending => self.pending += 1,
            Drawn::Failed if !self.failed.iter().any(|f| f == id) => self.failed.push(id.to_string()),
            _ => {}
        }
    }
}

/// One card. Returns its rect and `Some(double)` when it was clicked (double-clicked).
fn card(app: &mut CraftApp, ui: &mut Ui, c: &Card, recent: bool, tally: &mut Tally) -> (Rect, Option<bool>) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(CARD, Sense::click());
    let spoken = if recent { format!("{} (recent)", c.label) } else { c.label.clone() };
    let on = app.gallery.selected.as_deref() == Some(c.id.as_str());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, &spoken));
    let fill = if resp.hovered() { t.hover } else { t.panel };
    ui.painter().rect(r, 6.0, fill, Stroke::new(if on { 2.0 } else { 1.0 }, if on { t.accent } else { t.border }), egui::StrokeKind::Inside);
    let pr = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(r.width() - 16.0, PREVIEW_H));
    preview(app, ui, pr, c, &Choices::default(), CARD_SHAPES, tally);
    let clip = ui.painter().with_clip_rect(r.shrink2(vec2(8.0, 0.0)));
    clip.text(pos2(r.min.x + 10.0, r.min.y + PREVIEW_H + 20.0), Align2::LEFT_CENTER, &c.label, medium(13.0), t.text);
    clip.text(pos2(r.min.x + 10.0, r.min.y + PREVIEW_H + 38.0), Align2::LEFT_CENTER, &c.description, regular(11.0), t.text_dim);
    let mut x = r.min.x + 10.0;
    let mut chips: Vec<String> = Vec::new();
    for f in &c.code_forms {
        let name = code_form_name(f);
        if !chips.contains(&name) {
            chips.push(name);
        }
    }
    if c.key {
        chips.push("Key".into());
    }
    if c.title_block {
        chips.push("Title block".into());
    }
    for chip in chips {
        let g = clip.layout_no_wrap(chip, regular(10.0), t.text_dim);
        let cr = Rect::from_min_size(pos2(x, r.max.y - 22.0), vec2(g.size().x + 10.0, 16.0));
        clip.rect(cr, 8.0, t.ribbon, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        clip.galley(cr.min + vec2(5.0, (16.0 - g.size().y) / 2.0), g, t.text_dim);
        x = cr.max.x + 4.0;
    }
    let got = if resp.double_clicked() {
        Some(true)
    } else if resp.clicked() {
        Some(false)
    } else {
        None
    };
    (r, got)
}

/// Shapes a card's preview shows in its row; the details panel shows a few more.
const CARD_SHAPES: usize = 5;

/// The preview of card `c` in `r` with `choices`: an example's drawing, a document type's empty
/// page with its first shapes, or the placeholder of a blank page card.
fn preview(app: &CraftApp, ui: &mut Ui, r: Rect, c: &Card, choices: &Choices, shapes: usize, tally: &mut Tally) {
    let t = Tokens::get(ui.ctx());
    if c.blank_viewpoint.is_some() {
        previews::placeholder(ui, r, &c.pack_name, &t);
    } else if c.example {
        tally.count(&c.id, previews::draw(ui, r, &c.id, choices, &bar(app), &c.pack_name));
    } else {
        tally.count(&c.id, previews::draw_type(ui, r, &c.id, choices, &bar(app), &c.pack_name, shapes));
    }
}

/// A code form id as a chip: `mermaid.flowchart` is Mermaid, `dbml` is DBML.
fn code_form_name(id: &str) -> String {
    let head = id.split('.').next().unwrap_or(id);
    match head {
        "dbml" => "DBML".into(),
        "plantuml" => "PlantUML".into(),
        "d2" => "D2".into(),
        other => title_case(other),
    }
}

/// The details panel of the selected card: a larger preview, the description, keywords and code
/// forms, the Notation and Sheet choices, the unsaved-changes notice and Create.
fn details(app: &mut CraftApp, ui: &mut Ui, c: &Card, rects: &mut Rects, tally: &mut Tally) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).corner_radius(6.0).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(DETAILS_W - 26.0);
        ui.label(egui::RichText::new(&c.label).font(semibold(16.0)).color(t.text));
        ui.add_space(6.0);
        let (pr, _) = ui.allocate_exact_size(vec2(DETAILS_W - 26.0, 190.0), Sense::hover());
        let chosen = Choices { viewpoint: app.gallery.viewpoint.clone(), variant: app.gallery.variant.clone(), sheet: app.gallery.sheet.clone() };
        preview(app, ui, pr, c, &chosen, CARD_SHAPES + 1, tally);
        ui.add_space(6.0);
        if !c.description.is_empty() {
            ui.label(egui::RichText::new(&c.description).font(regular(12.0)).color(t.text));
        }
        if !c.keywords.is_empty() {
            ui.label(egui::RichText::new(format!("Keywords: {}", c.keywords.join(", "))).font(regular(11.0)).color(t.text_dim));
        }
        if !c.code_forms.is_empty() {
            let mut names: Vec<String> = c.code_forms.iter().map(|f| code_form_name(f)).collect();
            names.dedup();
            ui.label(egui::RichText::new(format!("Code: {}", names.join(", "))).font(regular(11.0)).color(t.text_dim));
        }
        ui.add_space(8.0);
        if c.blank_viewpoint.is_none() {
            choices(app, ui, c);
            let vp = app.gallery.viewpoint.clone().unwrap_or_else(|| c.viewpoint.clone());
            let rules = c.viewpoints.iter().find(|x| x.id == vp).map(|x| x.rulesets.clone()).unwrap_or_default();
            if !rules.is_empty() {
                ui.label(egui::RichText::new(format!("Checks: {}", rules.join(", "))).font(regular(11.0)).color(t.text_dim));
            }
        }
        if app.gallery.notice.as_deref() == Some(c.id.as_str()) {
            ui.add_space(6.0);
            egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.red)).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
                ui.label(egui::RichText::new(NOTICE).font(regular(12.0)).color(t.text));
                ui.horizontal(|ui| {
                    for label in ["Save First", "Replace", "Cancel"] {
                        let r = ui.button(label);
                        rects.buttons.push((label.into(), r.rect));
                        if r.clicked() {
                            match label {
                                "Save First" => save_first(app, &c.id),
                                "Replace" => {
                                    let _ = create(app, &c.id, true);
                                }
                                _ => app.gallery.notice = None,
                            }
                        }
                    }
                });
            });
        }
        ui.add_space(10.0);
        let r = crate::widgets::primary_button(ui, "Create").on_hover_text("Create (Enter)");
        rects.buttons.push(("Create".into(), r.rect));
        if r.clicked() {
            let _ = create(app, &c.id, false);
        }
    });
}

/// The Viewpoint combo (when the type offers several), the Notation combo (when the pack has
/// variants) and the Sheet combo (when the type draws a sheet). Each starts on what the engine
/// would choose; only a change is sent.
fn choices(app: &mut CraftApp, ui: &mut Ui, c: &Card) {
    let st = bar(app);
    let label_of = |app: &CraftApp, id: &str| app.gallery.presets.iter().find(|p| p.id == id).map_or(id.to_string(), |p| p.label.clone());
    if c.viewpoints.len() > 1 {
        let current = app.gallery.viewpoint.clone().unwrap_or_else(|| c.viewpoint.clone());
        let name = c.viewpoints.iter().find(|v| v.id == current).map_or(current.clone(), |v| v.label.clone());
        let mut picked: Option<String> = None;
        labelled_combo(ui, "Viewpoint", "gallery_viewpoint", name, |ui| {
            for v in &c.viewpoints {
                if ui.selectable_label(current == v.id, &v.label).clicked() {
                    picked = Some(v.id.clone());
                }
            }
        });
        if let Some(v) = picked {
            app.gallery.viewpoint = (v != c.viewpoint).then_some(v);
        }
    }
    if !c.variants.is_empty() {
        let engine = template::resolved(&c.id, &Choices::default(), Some(&st)).map(|(v, _)| v).unwrap_or_default();
        let current = app.gallery.variant.clone().unwrap_or(engine);
        let name = |v: &str| {
            if v.is_empty() {
                "Default drawing".to_string()
            } else {
                c.variants.iter().find(|(id, _)| id == v).map_or(v.to_string(), |(_, l)| l.clone())
            }
        };
        let mut picked: Option<String> = None;
        labelled_combo(ui, "Notation", "gallery_notation", name(&current), |ui| {
            for (id, label) in &c.variants {
                if ui.selectable_label(current == *id, label).clicked() {
                    picked = Some(id.clone());
                }
            }
            if ui.selectable_label(current.is_empty(), "Default drawing").clicked() {
                picked = Some(String::new());
            }
        });
        if let Some(v) = picked
            && v != current
        {
            app.gallery.variant = Some(v);
        }
    }
    if !c.sheet.is_empty() {
        let project = format!("Project sheet ({})", label_of(app, &st.sheet));
        let current = match app.gallery.sheet.as_deref() {
            None => project.clone(),
            Some("") => "No sheet".to_string(),
            Some(id) => label_of(app, id),
        };
        let mut picked: Option<Option<String>> = None;
        let presets = app.gallery.presets.clone();
        labelled_combo(ui, "Sheet", "gallery_template_sheet", current, |ui| {
            if ui.selectable_label(app.gallery.sheet.is_none(), &project).clicked() {
                picked = Some(None);
            }
            for p in &presets {
                if ui.selectable_label(app.gallery.sheet.as_deref() == Some(p.id.as_str()), &p.label).clicked() {
                    picked = Some(Some(p.id.clone()));
                }
            }
            if ui.selectable_label(app.gallery.sheet.as_deref() == Some(""), "No sheet").clicked() {
                picked = Some(Some(String::new()));
            }
        });
        if let Some(s) = picked {
            app.gallery.sheet = s;
        }
    }
}

/// A combo the width of the details panel with its name above it (painted; the combo itself
/// carries the name for screen readers and tests).
fn labelled_combo(ui: &mut Ui, name: &str, salt: &str, text: String, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::get(ui.ctx());
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, name, regular(11.5), t.text_dim);
    let resp = egui::ComboBox::from_id_salt(salt).selected_text(text).width(ui.available_width() - 4.0).truncate().show_ui(ui, add);
    resp.response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, name));
    ui.add_space(4.0);
}
