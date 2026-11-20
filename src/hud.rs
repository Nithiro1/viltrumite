//! The HUD and the ability menu (design/hud-menu: 1-imagem.png, and the numbered parts of
//! spec.md). Layout and the menu's drag and drop are pure and tested; drawing goes through the
//! game's HUD helpers (`fill`, `text`, `item`) in GUI pixels.
use crate::abilities::{Ability, Loadout, SLOTS};
use mod_api::{DrawContext, ItemKey, Registrar};
use std::collections::HashMap;
use std::sync::OnceLock;

/// GLFW's mouse buttons, as the screen hooks give them.
pub const LEFT: i32 = 0;
pub const RIGHT: i32 = 1;

/// A rectangle in GUI pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[0] < self.x + self.w && p[1] >= self.y && p[1] < self.y + self.h
    }

    fn arr(&self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }
}

/// The item each ability shows (a native mod carries no images): 16, and the impact's.
pub fn icon(ability: Ability) -> &'static str {
    match ability {
        Ability::Dash => "minecraft:feather",
        Ability::Grab => "minecraft:lead",
        Ability::Clap => "minecraft:goat_horn",
    }
}
pub const IMPACT_ICON: &str = "minecraft:anvil";

/// The icons' items, asked for once at registration (the API draws items by key).
static ICONS: OnceLock<HashMap<&'static str, ItemKey>> = OnceLock::new();

pub fn register(r: &mut dyn Registrar) {
    let choice = [VILTRUMITE_ICON, HUMAN_ICON].into_iter().chain(POWERS.map(|(item, _)| item));
    let ids = Ability::ALL.map(icon).into_iter().chain([IMPACT_ICON]).chain(choice);
    let _ = ICONS.set(ids.map(|id| (id, r.item_key(id))).collect());
}

// The choice on joining a world (design/escolha: 1-imagem.png and spec.md, parts 1-12).

/// Which card of the choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Viltrumite,
    Human,
}

/// 6 and 9: the cards' items.
const VILTRUMITE_ICON: &str = "minecraft:lightning_rod";
const HUMAN_ICON: &str = "minecraft:player_head";
/// 7: the powers, each with its item.
const POWERS: [(&str, &str); 4] = [
    ("minecraft:feather", "Voo até o sônico (pulo duplo)"),
    ("minecraft:mace", "Força e empurrão 4x"),
    ("minecraft:shield", "Toma só 20% do dano"),
    ("minecraft:goat_horn", "Dash, agarrar, palmas, impacto"),
];
/// 10: staying human.
const HUMAN_LINES: [&str; 3] = ["Sem voo nem HUD", "Dano e força normais", "Dá pra virar Viltrumita depois"];

pub const CHOICE_W: f32 = 400.0;
pub const CHOICE_H: f32 = 184.0;
const CHOICE_CARD_H: f32 = 122.0;

/// Where the choice's parts are for a screen `gui` big, the panel in the middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChoiceLayout {
    /// 2.
    pub panel: Rect,
    /// 5 and 8.
    pub viltrumite: Rect,
    pub human: Rect,
}

impl ChoiceLayout {
    pub fn new(gui: [f32; 2]) -> ChoiceLayout {
        let x = ((gui[0] - CHOICE_W) / 2.0).max(0.0).floor();
        let y = ((gui[1] - CHOICE_H) / 2.0).max(0.0).floor();
        let card_w = (CHOICE_W - 10.0 - 10.0 - 8.0) / 2.0;
        ChoiceLayout {
            panel: Rect::new(x, y, CHOICE_W, CHOICE_H),
            viltrumite: Rect::new(x + 10.0, y + 38.0, card_w, CHOICE_CARD_H),
            human: Rect::new(x + 10.0 + card_w + 8.0, y + 38.0, card_w, CHOICE_CARD_H),
        }
    }

    /// The card under `p`, if any.
    pub fn at(&self, p: [f32; 2]) -> Option<Choice> {
        if self.viltrumite.contains(p) {
            Some(Choice::Viltrumite)
        } else if self.human.contains(p) {
            Some(Choice::Human)
        } else {
            None
        }
    }
}

/// The choice for this frame (1-12).
pub fn draw_choice(draw: &mut dyn DrawContext, gui: [f32; 2], mouse: [f32; 2]) {
    let l = ChoiceLayout::new(gui);
    draw.fill([0.0, 0.0, gui[0], gui[1]], 0xA8080A0E);
    draw.fill(l.panel.arr(), 0xF2232833);
    outline(draw, l.panel, 0xFF4A5263);
    let (x, y) = (l.panel.x, l.panel.y);
    text(draw, "VOCÊ É UM VILTRUMITA?", [x + 10.0, y + 9.0], GOLD, true);
    text(draw, "Escolha como quer jogar neste mundo.", [x + 10.0, y + 22.0], GREY, false);

    let hover = l.at(mouse);
    let lines: Vec<(Option<&str>, &str)> = POWERS.iter().map(|&(item, line)| (Some(item), line)).collect();
    choice_card(draw, l.viltrumite, VILTRUMITE_ICON, "Ser um Viltrumita", "Comece já com todos os poderes.", &lines, hover == Some(Choice::Viltrumite));
    let lines: Vec<(Option<&str>, &str)> = HUMAN_LINES.iter().map(|&line| (None, line)).collect();
    choice_card(draw, l.human, HUMAN_ICON, "Continuar humano", "Minecraft normal, sem poderes.", &lines, hover == Some(Choice::Human));

    let foot = y + CHOICE_H - 16.0;
    let mut at = x + 10.0;
    for (part, colour) in [("Mudar depois: ", GREY), ("/viltrumite power", GOLD), (" ou ", GREY), ("depower", GOLD)] {
        text(draw, part, [at, foot], colour, false);
        at += draw.text_width(part);
    }
    let esc = "Esc: decidir depois";
    let tw = draw.text_width(esc);
    text(draw, esc, [x + CHOICE_W - 10.0 - tw, foot], GREY, false);
}

/// 5 or 8: the icon, the name, a line about it and the list; gold-edged under the mouse.
fn choice_card(draw: &mut dyn DrawContext, r: Rect, item: &str, name: &str, about: &str, lines: &[(Option<&str>, &str)], hover: bool) {
    draw.fill(r.arr(), if hover { 0xFF353D4D } else { 0xFF2C3341 });
    outline(draw, r, if hover { 0xFF000000 | GOLD } else { 0xFF3D4556 });
    draw.fill([r.x + 6.0, r.y + 6.0, 24.0, 24.0], 0xFF141820);
    draw_item(draw, item, [r.x + 10.0, r.y + 10.0], 16.0);
    text(draw, name, [r.x + 6.0, r.y + 36.0], WHITE, true);
    text(draw, about, [r.x + 6.0, r.y + 48.0], GREY, false);
    for (i, (icon, line)) in lines.iter().enumerate() {
        let ly = r.y + 64.0 + i as f32 * 13.0;
        match icon {
            Some(icon) => draw_item(draw, icon, [r.x + 6.0, ly - 1.0], 10.0),
            None => draw.fill([r.x + 8.0, ly + 2.0, 5.0, 5.0], 0xFF9AA4B2),
        }
        text(draw, line, [r.x + 19.0, ly], 0xCFD6E2, false);
    }
}

/// Item `id`'s icon, if it was asked for.
fn draw_item(draw: &mut dyn DrawContext, id: &str, at: [f32; 2], size: f32) {
    if let Some(&key) = ICONS.get().and_then(|icons| icons.get(id)) {
        draw.item(key, at, size);
    }
}

/// A line of text; a colour given as `0xRRGGBB` is drawn opaque (the game skips alpha 0).
fn text(draw: &mut dyn DrawContext, line: &str, at: [f32; 2], color: u32, shadow: bool) {
    let argb = if color >> 24 == 0 { 0xFF00_0000 | color } else { color };
    draw.text(line, at, argb, shadow);
}

/// The menu's short descriptions and their cooldowns.
pub fn about(ability: Ability) -> (&'static str, &'static str) {
    match ability {
        Ability::Dash => ("Arranque para onde você olha", "recarga 1 s"),
        Ability::Grab => ("Segura o mob à frente; de novo arremessa", "sem recarga"),
        Ability::Clap => ("Onda de choque em cone à frente", "recarga 3 s"),
    }
}

// ---- HUD (spec 1-13) ----

/// Size of a HUD slot (9-12) and the gap between them.
pub const SLOT: f32 = 22.0;
const GAP: f32 = 2.0;
/// The vanilla hotbar is 182 wide, centred, 22 tall at the bottom.
const HOTBAR_HALF: f32 = 91.0;

/// The four slots, a column right of the hotbar (past the attack indicator), slot 4 lowest.
pub fn slot_column(gui: [f32; 2]) -> [Rect; SLOTS] {
    let x = gui[0] / 2.0 + HOTBAR_HALF + 26.0;
    let bottom = gui[1] - 2.0;
    std::array::from_fn(|i| {
        let from_bottom = (SLOTS - i) as f32;
        Rect::new(x, bottom - from_bottom * SLOT - (from_bottom - 1.0) * GAP, SLOT, SLOT)
    })
}

/// How full the speed bar (5) is, 0..=1.
pub fn speed_fill(speed: f64, max: f64) -> f32 {
    if max <= 0.0 { 0.0 } else { (speed / max).clamp(0.0, 1.0) as f32 }
}

/// How much of a slot the cooldown veil (13) covers, 0..=1.
pub fn veil(left: u32, total: u32) -> f32 {
    if total == 0 { 0.0 } else { (left as f32 / total as f32).clamp(0.0, 1.0) }
}

/// Whole seconds left, rounded up, for the veil's number.
pub fn seconds(left: u32) -> u32 {
    left.div_ceil(20)
}

/// What the HUD shows this frame.
pub struct HudView<'a> {
    pub gui: [f32; 2],
    pub mode: &'a str,
    pub sonic: bool,
    pub speed_mps: f64,
    pub speed_fill: f32,
    pub sonic_mark: f32,
    pub holding: bool,
    pub loadout: Loadout,
    pub keys: [String; SLOTS],
    pub menu_key: String,
    pub show_hint: bool,
    /// Ticks left and total cooldown of each slot's ability (0, 0 when ready or none).
    pub cooldowns: [(u32, u32); SLOTS],
    /// Slots that just became ready: a white flash.
    pub flash: [bool; SLOTS],
}

const PANEL: u32 = 0x8C00_0000;
const WHITE: u32 = 0xFFFFFF;
const GOLD: u32 = 0xF2C12E;
const CYAN: u32 = 0x7FE3FF;
const GREY: u32 = 0xAAB3C2;
const PINK: u32 = 0xFFB0B0;

/// The status card (1-7) in the top-left corner at `at`, and the slots (8-13).
pub fn draw_hud(draw: &mut dyn DrawContext, at: (f32, f32), v: &HudView<'_>) {
    let (x, y) = at;
    let card = Rect::new(x, y, CARD_W, card_height(v.holding));
    draw.fill(card.arr(), PANEL);
    text(draw, "VILTRUMITA", [x + 5.0, y + 4.0], GOLD, true);
    text(draw, v.mode, [x + 5.0, y + 15.0], if v.sonic { CYAN } else { WHITE }, true);
    let speed = format!("{:.0} m/s", v.speed_mps);
    let tw = draw.text_width(&speed);
    text(draw, &speed, [x + CARD_W - 5.0 - tw, y + 15.0], CYAN, true);
    let bar = Rect::new(x + 5.0, y + 26.0, CARD_W - 10.0, 3.0);
    draw.fill(bar.arr(), 0xFF2A2F3A);
    draw.fill([bar.x, bar.y, bar.w * v.speed_fill, bar.h], 0xFF000000 | CYAN);
    draw.fill([bar.x + bar.w * v.sonic_mark - 0.5, bar.y - 1.5, 1.0, bar.h + 3.0], 0xFF000000 | GOLD);
    if v.holding {
        draw_item(draw, icon(Ability::Grab), [x + 4.0, y + 31.0], 9.0);
        text(draw, "Segurando um mob", [x + 15.0, y + 32.0], PINK, true);
    }

    let slots = slot_column(v.gui);
    if v.show_hint {
        let hint = format!("{} · habilidades", v.menu_key);
        let w = draw.text_width(&hint) + 6.0;
        let top = slots[0];
        draw.fill([top.x + SLOT - w, top.y - 13.0, w, 11.0], PANEL);
        text(draw, &hint, [top.x + SLOT - w + 3.0, top.y - 11.5], WHITE, true);
    }
    for (i, r) in slots.iter().enumerate() {
        let ability = v.loadout.slot(i);
        draw.fill(r.arr(), 0x99000000);
        match ability {
            Some(a) => {
                outline(draw, *r, 0xFF9AA4B2);
                let item = if a == Ability::Grab && v.holding { icon(Ability::Grab) } else { icon(a) };
                draw_item(draw, item, [r.x + 3.0, r.y + 3.0], 16.0);
                if a == Ability::Grab && v.holding {
                    draw.fill([r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0], 0x55FF5050);
                }
            }
            None => {
                dashed(draw, *r, 0xFF6B7380);
                text(draw, "+", [r.x + 8.5, r.y + 7.0], 0x6B7380, false);
            }
        }
        let (left, total) = v.cooldowns[i];
        let covered = veil(left, total);
        if covered > 0.0 {
            let h = (r.h - 2.0) * covered;
            draw.fill([r.x + 1.0, r.y + r.h - 1.0 - h, r.w - 2.0, h], 0xB80A0C12);
            let s = seconds(left).to_string();
            let tw = draw.text_width(&s);
            text(draw, &s, [r.x + (r.w - tw) / 2.0, r.y + 7.0], WHITE, true);
        }
        if v.flash[i] {
            draw.fill(r.arr(), 0x66FFFFFF);
        }
        let key = short_key(&v.keys[i]);
        text(draw, &key, [r.x + 1.5, r.y + 1.0], GOLD, true);
    }
}

pub const CARD_W: f32 = 120.0;

pub fn card_height(holding: bool) -> f32 {
    if holding { 43.0 } else { 33.0 }
}

/// `key.keyboard.left.control` to `CTRL`, `key.keyboard.v` to `V`, a mouse button to `M4`.
pub fn short_key(key: &str) -> String {
    let name = key.rsplit('.').next().unwrap_or(key);
    if let Some(n) = key.strip_prefix("key.mouse.") {
        return format!("M{}", n.parse::<u32>().map_or(n.to_uppercase(), |n| (n + 1).to_string()));
    }
    match name {
        "control" => "CTRL".to_owned(),
        "shift" => "SHIFT".to_owned(),
        "alt" => "ALT".to_owned(),
        n if n.chars().count() <= 3 => n.to_uppercase(),
        n => n.chars().take(3).collect::<String>().to_uppercase(),
    }
}

fn outline(draw: &mut dyn DrawContext, r: Rect, argb: u32) {
    draw.fill([r.x, r.y, r.w, 1.0], argb);
    draw.fill([r.x, r.y + r.h - 1.0, r.w, 1.0], argb);
    draw.fill([r.x, r.y, 1.0, r.h], argb);
    draw.fill([r.x + r.w - 1.0, r.y, 1.0, r.h], argb);
}

fn dashed(draw: &mut dyn DrawContext, r: Rect, argb: u32) {
    let mut t = 0.0;
    while t < r.w {
        let len = 2.0f32.min(r.w - t);
        draw.fill([r.x + t, r.y, len, 1.0], argb);
        draw.fill([r.x + t, r.y + r.h - 1.0, len, 1.0], argb);
        t += 4.0;
    }
    let mut t = 0.0;
    while t < r.h {
        let len = 2.0f32.min(r.h - t);
        draw.fill([r.x, r.y + t, 1.0, len], argb);
        draw.fill([r.x + r.w - 1.0, r.y + t, 1.0, len], argb);
        t += 4.0;
    }
}

// ---- The ability menu (spec 14-22) ----

pub const MENU_W: f32 = 300.0;
pub const MENU_H: f32 = 232.0;
const ROW_H: f32 = 26.0;

/// Where everything of the menu is, for a window of `gui` GUI pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuLayout {
    pub panel: Rect,
    /// 16: one card an ability, in `Ability::ALL`'s order; then 17, the impact.
    pub cards: [Rect; 3],
    pub impact: Rect,
    /// 18.
    pub slots: [Rect; SLOTS],
    /// 21 and 22.
    pub reset: Rect,
    pub close: Rect,
}

impl MenuLayout {
    pub fn new(gui: [f32; 2]) -> MenuLayout {
        let x = ((gui[0] - MENU_W) / 2.0).max(0.0).floor();
        let y = ((gui[1] - MENU_H) / 2.0).max(0.0).floor();
        let row = |i: usize| Rect::new(x + 8.0, y + 36.0 + i as f32 * (ROW_H + 2.0), MENU_W - 16.0, ROW_H);
        let slot_w = 64.0;
        let slot_x0 = x + (MENU_W - (4.0 * slot_w + 3.0 * 6.0)) / 2.0;
        MenuLayout {
            panel: Rect::new(x, y, MENU_W, MENU_H),
            cards: [row(0), row(1), row(2)],
            impact: row(3),
            slots: std::array::from_fn(|i| Rect::new(slot_x0 + i as f32 * (slot_w + 6.0), y + 160.0, slot_w, 44.0)),
            reset: Rect::new(x + MENU_W - 8.0 - 60.0 - 6.0 - 92.0, y + MENU_H - 22.0, 92.0, 16.0),
            close: Rect::new(x + MENU_W - 8.0 - 60.0, y + MENU_H - 22.0, 60.0, 16.0),
        }
    }

    pub fn card_at(&self, p: [f32; 2]) -> Option<Ability> {
        self.cards.iter().position(|r| r.contains(p)).map(|i| Ability::ALL[i])
    }

    pub fn slot_at(&self, p: [f32; 2]) -> Option<usize> {
        self.slots.iter().position(|r| r.contains(p))
    }
}

/// What is being dragged (19), and from where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    Card(Ability),
    Slot(usize, Ability),
}

impl Drag {
    pub fn ability(self) -> Ability {
        match self {
            Drag::Card(a) | Drag::Slot(_, a) => a,
        }
    }
}

/// What a click asks of the client beyond the menu itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    /// The loadout changed: save it.
    Changed,
    Close,
}

/// The menu's own state between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    pub drag: Option<Drag>,
}

impl Menu {
    pub fn press(&mut self, layout: &MenuLayout, mouse: [f32; 2], button: i32, loadout: &mut Loadout) -> MenuAction {
        if button == RIGHT {
            if let Some(slot) = layout.slot_at(mouse)
                && loadout.slot(slot).is_some()
            {
                loadout.set(slot, None);
                return MenuAction::Changed;
            }
            return MenuAction::None;
        }
        if button != LEFT {
            return MenuAction::None;
        }
        if layout.close.contains(mouse) {
            return MenuAction::Close;
        }
        if layout.reset.contains(mouse) {
            *loadout = Loadout::default();
            return MenuAction::Changed;
        }
        if let Some(ability) = layout.card_at(mouse) {
            self.drag = Some(Drag::Card(ability));
        } else if let Some(slot) = layout.slot_at(mouse)
            && let Some(ability) = loadout.slot(slot)
        {
            self.drag = Some(Drag::Slot(slot, ability));
        }
        MenuAction::None
    }

    /// The button let go: a drag lands in the slot under the mouse, if any.
    pub fn release(&mut self, layout: &MenuLayout, mouse: [f32; 2], loadout: &mut Loadout) -> MenuAction {
        let Some(drag) = self.drag.take() else {
            return MenuAction::None;
        };
        let Some(target) = layout.slot_at(mouse) else {
            return MenuAction::None;
        };
        let before = *loadout;
        match drag {
            Drag::Card(ability) => loadout.set(target, Some(ability)),
            Drag::Slot(from, ability) => {
                // Swap: what was in the target goes where the dragged one came from.
                let displaced = loadout.slot(target);
                loadout.set(target, Some(ability));
                if from != target {
                    loadout.set(from, displaced.filter(|d| *d != ability));
                }
            }
        }
        if *loadout == before { MenuAction::None } else { MenuAction::Changed }
    }
}

/// The menu for this frame (14-22).
pub fn draw_menu(draw: &mut dyn DrawContext, gui: [f32; 2], mouse: [f32; 2], menu: &Menu, loadout: &Loadout, keys: &[String; SLOTS], menu_key: &str) {
    let l = MenuLayout::new(gui);
    draw.fill([0.0, 0.0, gui[0], gui[1]], 0x9E080A0E);
    draw.fill(l.panel.arr(), 0xF2232833);
    outline(draw, l.panel, 0xFF4A5263);
    let (x, y) = (l.panel.x, l.panel.y);
    text(draw, "HABILIDADES", [x + 8.0, y + 7.0], GOLD, true);
    text(draw, "Arraste para um slot · botão direito esvazia", [x + 8.0, y + 19.0], GREY, false);

    for (i, r) in l.cards.iter().enumerate() {
        let ability = Ability::ALL[i];
        let hover = r.contains(mouse) && menu.drag.is_none();
        card(draw, *r, icon(ability), ability.name(), about(ability).0, about(ability).1, hover, false);
        if let Some(slot) = (0..SLOTS).find(|&s| loadout.slot(s) == Some(ability)) {
            let tag = format!("slot {}", slot + 1);
            let tw = draw.text_width(&tag);
            text(draw, &tag, [r.x + r.w - 4.0 - tw, r.y + 15.0], GOLD, false);
        }
    }
    card(draw, l.impact, IMPACT_ICON, "Impacto", "Automático: mergulhe no chão voando rápido", "passivo", false, true);

    text(draw, "SLOTS", [x + 8.0, y + 149.0], GREY, false);
    for (i, r) in l.slots.iter().enumerate() {
        let target = menu.drag.is_some() && r.contains(mouse);
        draw.fill(r.arr(), if target { 0xFF1D222C } else { 0xFF141820 });
        if target {
            dashed(draw, *r, 0xFF000000 | GOLD);
        } else {
            outline(draw, *r, 0xFF4A5263);
        }
        text(draw, &short_key(&keys[i]), [r.x + 4.0, r.y + 3.0], GOLD, true);
        match loadout.slot(i) {
            Some(a) if menu.drag != Some(Drag::Slot(i, a)) => {
                draw_item(draw, icon(a), [r.x + (r.w - 16.0) / 2.0, r.y + 10.0], 16.0);
                let name = slot_name(a);
                let tw = draw.text_width(name);
                text(draw, name, [r.x + (r.w - tw) / 2.0, r.y + 31.0], 0xCFD6E2, false);
            }
            _ => {
                let label = if target { "solte aqui" } else { "vazio" };
                let tw = draw.text_width(label);
                text(draw, label, [r.x + (r.w - tw) / 2.0, r.y + 18.0], if target { GOLD } else { 0x6B7380 }, false);
            }
        }
    }

    text(draw, "Teclas: Opções › Controles", [x + 8.0, y + MENU_H - 18.0], GREY, false);
    button(draw, l.reset, "Restaurar padrão", l.reset.contains(mouse));
    let close = format!("Fechar ({menu_key})");
    button(draw, l.close, &close, l.close.contains(mouse));

    if let Some(drag) = menu.drag {
        let a = drag.ability();
        let r = Rect::new(mouse[0] - 10.0, mouse[1] - 8.0, 150.0, ROW_H);
        card(draw, r, icon(a), a.name(), "", "", true, false);
        outline(draw, r, 0xFF000000 | GOLD);
    }
}

fn slot_name(a: Ability) -> &'static str {
    match a {
        Ability::Dash => "Dash",
        Ability::Grab => "Agarrar",
        Ability::Clap => "Palmas",
    }
}

#[allow(clippy::too_many_arguments)]
fn card(draw: &mut dyn DrawContext, r: Rect, item: &str, name: &str, about: &str, cooldown: &str, hover: bool, passive: bool) {
    draw.fill(r.arr(), if hover { 0xFF353D4D } else { 0xFF2C3341 });
    outline(draw, r, if hover { 0xFF000000 | GOLD } else { 0xFF3D4556 });
    draw.fill([r.x + 3.0, r.y + 3.0, 20.0, 20.0], 0xFF141820);
    draw_item(draw, item, [r.x + 5.0, r.y + 5.0], 16.0);
    let name_colour = if passive { GREY } else { WHITE };
    text(draw, name, [r.x + 28.0, r.y + 4.0], name_colour, false);
    if !about.is_empty() {
        text(draw, about, [r.x + 28.0, r.y + 15.0], GREY, false);
    }
    if !cooldown.is_empty() {
        let tw = draw.text_width(cooldown);
        text(draw, cooldown, [r.x + r.w - 4.0 - tw, r.y + 4.0], GREY, false);
    }
}

fn button(draw: &mut dyn DrawContext, r: Rect, label: &str, hover: bool) {
    draw.fill(r.arr(), if hover { 0xFF4A546A } else { 0xFF3A4253 });
    outline(draw, r, 0xFF56607A);
    let tw = draw.text_width(label);
    text(draw, label, [r.x + (r.w - tw) / 2.0, r.y + 4.0], WHITE, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_sits_in_the_middle_and_finds_its_cards() {
        let l = ChoiceLayout::new([640.0, 360.0]);
        assert_eq!((l.panel.x, l.panel.y), (120.0, 88.0));
        let inside = |r: Rect| r.x >= l.panel.x && r.x + r.w <= l.panel.x + l.panel.w && r.y + r.h <= l.panel.y + CHOICE_H - 18.0;
        assert!(inside(l.viltrumite) && inside(l.human), "cards inside, above the footer");
        assert!(l.viltrumite.x + l.viltrumite.w < l.human.x, "side by side");
        assert_eq!(l.at([l.viltrumite.x + 5.0, l.viltrumite.y + 5.0]), Some(Choice::Viltrumite));
        assert_eq!(l.at([l.human.x + l.human.w - 1.0, l.human.y + 60.0]), Some(Choice::Human));
        assert_eq!(l.at([l.panel.x + 2.0, l.panel.y + 2.0]), None, "the title isn't a card");
        assert_eq!(ChoiceLayout::new([300.0, 150.0]).panel.x, 0.0, "a small screen keeps it on screen");
    }

    #[test]
    fn hud_slots_stand_right_of_the_hotbar_bottom_up() {
        let s = slot_column([427.0, 240.0]);
        assert!(s.iter().all(|r| r.x > 427.0 / 2.0 + 91.0));
        assert!((s[3].y + SLOT - 238.0).abs() < 1e-3, "slot 4 sits on the bottom");
        assert!(s.windows(2).all(|w| w[0].y + SLOT + GAP == w[1].y), "a column, top to bottom");
    }

    #[test]
    fn bars_and_veils_fill_their_share() {
        assert_eq!(speed_fill(2.0, 4.0), 0.5);
        assert_eq!(speed_fill(9.0, 4.0), 1.0);
        assert_eq!(speed_fill(1.0, 0.0), 0.0);
        assert_eq!(veil(30, 60), 0.5);
        assert_eq!(veil(0, 60), 0.0);
        assert_eq!(veil(5, 0), 0.0);
        assert_eq!(seconds(41), 3);
        assert_eq!(seconds(20), 1);
        assert_eq!(seconds(0), 0);
    }

    #[test]
    fn keys_read_short() {
        assert_eq!(short_key("key.keyboard.v"), "V");
        assert_eq!(short_key("key.keyboard.left.control"), "CTRL");
        assert_eq!(short_key("key.keyboard.f6"), "F6");
        assert_eq!(short_key("key.keyboard.semicolon"), "SEM");
        assert_eq!(short_key("key.mouse.3"), "M4");
    }

    #[test]
    fn the_menu_fits_a_small_window_and_its_parts_dont_overlap() {
        for gui in [[320.0, 240.0], [640.0, 360.0], [1280.0, 720.0]] {
            let l = MenuLayout::new(gui);
            let inside = |r: &Rect| r.x >= l.panel.x && r.y >= l.panel.y && r.x + r.w <= l.panel.x + l.panel.w && r.y + r.h <= l.panel.y + l.panel.h;
            assert!(l.cards.iter().chain([&l.impact, &l.reset, &l.close]).chain(l.slots.iter()).all(inside), "{gui:?}");
            assert!(l.impact.y + l.impact.h <= l.slots[0].y);
            assert!(l.slots[0].y + l.slots[0].h <= l.reset.y);
            assert!(l.reset.x + l.reset.w <= l.close.x);
        }
    }

    fn centre(r: Rect) -> [f32; 2] {
        [r.x + r.w / 2.0, r.y + r.h / 2.0]
    }

    #[test]
    fn dragging_a_card_into_a_slot_moves_it_there() {
        let l = MenuLayout::new([640.0, 360.0]);
        let mut menu = Menu::default();
        let mut loadout = Loadout::default();
        menu.press(&l, centre(l.cards[2]), LEFT, &mut loadout);
        assert_eq!(menu.drag, Some(Drag::Card(Ability::Clap)));
        assert_eq!(menu.release(&l, centre(l.slots[3]), &mut loadout), MenuAction::Changed);
        assert_eq!(loadout.encode(), "dash,grab,-,clap");
        assert_eq!(menu.drag, None);
    }

    #[test]
    fn dragging_between_slots_swaps_them() {
        let l = MenuLayout::new([640.0, 360.0]);
        let mut menu = Menu::default();
        let mut loadout = Loadout::default();
        menu.press(&l, centre(l.slots[0]), LEFT, &mut loadout);
        menu.release(&l, centre(l.slots[2]), &mut loadout);
        assert_eq!(loadout.encode(), "clap,grab,dash,-");
        menu.press(&l, centre(l.slots[1]), LEFT, &mut loadout);
        menu.release(&l, centre(l.slots[3]), &mut loadout);
        assert_eq!(loadout.encode(), "clap,-,dash,grab", "into an empty slot");
    }

    #[test]
    fn letting_go_outside_a_slot_changes_nothing() {
        let l = MenuLayout::new([640.0, 360.0]);
        let mut menu = Menu::default();
        let mut loadout = Loadout::default();
        menu.press(&l, centre(l.cards[0]), LEFT, &mut loadout);
        assert_eq!(menu.release(&l, [1.0, 1.0], &mut loadout), MenuAction::None);
        assert_eq!(loadout, Loadout::default());
        menu.press(&l, centre(l.cards[0]), LEFT, &mut loadout);
        assert_eq!(menu.release(&l, centre(l.slots[0]), &mut loadout), MenuAction::None, "already there");
    }

    #[test]
    fn right_click_empties_reset_restores_close_closes() {
        let l = MenuLayout::new([640.0, 360.0]);
        let mut menu = Menu::default();
        let mut loadout = Loadout::default();
        assert_eq!(menu.press(&l, centre(l.slots[1]), RIGHT, &mut loadout), MenuAction::Changed);
        assert_eq!(loadout.encode(), "dash,-,clap,-");
        assert_eq!(menu.press(&l, centre(l.slots[1]), RIGHT, &mut loadout), MenuAction::None, "already empty");
        assert_eq!(menu.press(&l, centre(l.reset), LEFT, &mut loadout), MenuAction::Changed);
        assert_eq!(loadout, Loadout::default());
        assert_eq!(menu.press(&l, centre(l.close), LEFT, &mut loadout), MenuAction::Close);
        assert_eq!(menu.press(&l, centre(l.impact), LEFT, &mut loadout), MenuAction::None, "the impact doesn't drag");
        assert_eq!(menu.drag, None);
    }
}
