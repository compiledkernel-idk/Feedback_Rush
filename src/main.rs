use macroquad::audio::{
    load_sound_from_bytes, play_sound, stop_sound, PlaySoundParams, Sound,
};
use macroquad::prelude::*;
use std::collections::VecDeque;

// -------------------------------
// Config
// -------------------------------
// Initial window size; all gameplay adapts to current window size at runtime
const WIDTH: f32 = 960.0;
const HEIGHT: f32 = 540.0;

const PLAYER_RADIUS: f32 = 12.0;
const GHOST_RADIUS: f32 = 10.0;
const ORB_RADIUS: f32 = 8.0;

const ACCEL: f32 = 1600.0;
const FRICTION: f32 = 5.5;
const MAX_SPEED: f32 = 300.0;

const FIXED_DT: f32 = 1.0 / 60.0;
const INPUT_HISTORY_SECONDS: f32 = 12.0;

const PHASE_MAX: f32 = 1.5;
const PHASE_DRAIN: f32 = 1.6; // per second
const PHASE_REGEN: f32 = 0.6; // per second

const ORB_SPAWN_BASE: f32 = 1.5; // seconds between spawns at start
const ORB_SPAWN_MIN: f32 = 0.35; // fastest spawn
const ORB_SAFE_RADIUS: f32 = 80.0; // avoid spawning on top of the player

const COMBO_DECAY_PER_SEC: f32 = 0.25;

// -------------------------------
// Game Data
// -------------------------------
#[derive(Clone, Copy, Debug)]
struct InputFrame {
    pos: Vec2,
}

#[derive(Clone)]
struct Ghost {
    samples: Vec<Vec2>,
    progress: f32, // measured in "frames"
    speed: f32,    // frames per second (1.0 == 60fps playback)
    radius: f32,
    color: Color,
    ttl: f32, // seconds to live
}

impl Ghost {
    fn current_pos(&self) -> Vec2 {
        if self.samples.is_empty() {
            return vec2(0.0, 0.0);
        }
        let n = self.samples.len() as f32;
        let mut p = self.progress % n.max(1.0);
        if p < 0.0 {
            p += n;
        }
        let i0 = p.floor() as usize;
        let i1 = (i0 + 1) % self.samples.len();
        let t = p.fract();
        self.samples[i0].lerp(self.samples[i1], t)
    }
}

struct Orb {
    pos: Vec2,
    radius: f32,
    alive: bool,
}

// -------------------------------
// Player
// -------------------------------
struct Player {
    pos: Vec2,
    vel: Vec2,
    radius: f32,
    phase_energy: f32,
    phase_active: bool,
}

impl Player {
    fn new(pos: Vec2) -> Self {
        Self {
            pos,
            vel: vec2(0.0, 0.0),
            radius: PLAYER_RADIUS,
            phase_energy: PHASE_MAX,
            phase_active: false,
        }
    }
}

// -------------------------------
// Sounds: tiny procedural WAVs
// -------------------------------
fn tone_wav(freq: f32, dur_s: f32, vol: f32, attack_s: f32, release_s: f32) -> Vec<u8> {
    let sr: u32 = 44100;
    let total = (dur_s * sr as f32) as usize;
    let attack = (attack_s * sr as f32) as usize;
    let release = (release_s * sr as f32) as usize;

    let mut samples = Vec::<i16>::with_capacity(total);
    for i in 0..total {
        let t = i as f32 / sr as f32;
        // Simple square+sine blend for a retro feel
        let sine = (2.0 * std::f32::consts::PI * freq * t).sin();
        let square = if sine >= 0.0 { 1.0 } else { -1.0 };
        let mut envelope = 1.0;
        if i < attack {
            envelope = i as f32 / attack as f32;
        } else if i >= total.saturating_sub(release) {
            let k = total - i;
            envelope = k as f32 / release as f32;
        }
        let s = ((0.5 * sine + 0.5 * square) * vol * envelope).clamp(-1.0, 1.0);
        samples.push((s * i16::MAX as f32) as i16);
    }
    // Build 16-bit PCM WAV
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let byte_rate = sr * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_len = (samples.len() * 2) as u32;
    let riff_chunk_size = 36 + data_len;

    let mut out = Vec::<u8>::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_chunk_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // fmt
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&num_channels.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[derive(Clone)]
struct AudioSet {
    collect: Sound,
    ghost_spawn: Sound,
    death: Sound,
    drone: Sound,
}

// -------------------------------
// Utils
// -------------------------------
fn clamp_rect(p: Vec2, r: f32, w: f32, h: f32) -> (Vec2, Vec2) {
    let mut pos = p;
    let mut norm = vec2(0.0, 0.0);
    if pos.x - r < 0.0 {
        pos.x = r;
        norm.x = 1.0;
    }
    if pos.x + r > w {
        pos.x = w - r;
        norm.x = -1.0;
    }
    if pos.y - r < 0.0 {
        pos.y = r;
        norm.y = 1.0;
    }
    if pos.y + r > h {
        pos.y = h - r;
        norm.y = -1.0;
    }
    (pos, norm)
}

fn circle_overlap(a: Vec2, ar: f32, b: Vec2, br: f32) -> bool {
    a.distance_squared(b) <= (ar + br) * (ar + br)
}

fn rand_pos_away_from(p: Vec2, min_dist: f32, w: f32, h: f32) -> Vec2 {
    use macroquad::rand::gen_range;
    for _ in 0..64 {
        let w1 = (w - 40.0).max(41.0);
        let h1 = (h - 40.0).max(41.0);
        let rp = vec2(gen_range(40.0, w1), gen_range(40.0, h1));
        if rp.distance(p) >= min_dist {
            return rp;
        }
    }
    vec2(
        clamp(p.x + 200.0, 40.0, (w - 40.0).max(40.0)),
        clamp(p.y + 150.0, 40.0, (h - 40.0).max(40.0)),
    )
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// -------------------------------
// Game State
// -------------------------------
#[derive(Clone, Copy)]
enum GameMode {
    Classic,
    TimeAttack,
    Nightmare,
}

impl GameMode {
    fn all() -> &'static [GameMode] {
        &[GameMode::Classic, GameMode::TimeAttack, GameMode::Nightmare]
    }
    fn name(&self) -> &'static str {
        match self {
            GameMode::Classic => "Classic",
            GameMode::TimeAttack => "Time Attack",
            GameMode::Nightmare => "Nightmare",
        }
    }
    fn index(&self) -> usize {
        match self {
            GameMode::Classic => 0,
            GameMode::TimeAttack => 1,
            GameMode::Nightmare => 2,
        }
    }
    fn from_index(i: usize) -> GameMode {
        match i % 3 {
            0 => GameMode::Classic,
            1 => GameMode::TimeAttack,
            _ => GameMode::Nightmare,
        }
    }
}

struct ModeConfig {
    time_limit: Option<f32>,
    ghost_speed_mul: f32,
    difficulty_rate: f32,
    spawn_rate_mul: f32,
    ghost_flicker: bool,
    ghost_invisible_far: bool,
}

fn mode_config(mode: GameMode) -> ModeConfig {
    match mode {
        GameMode::Classic => ModeConfig {
            time_limit: None,
            ghost_speed_mul: 1.0,
            difficulty_rate: 0.2,
            spawn_rate_mul: 1.0,
            ghost_flicker: false,
            ghost_invisible_far: false,
        },
        GameMode::TimeAttack => ModeConfig {
            time_limit: Some(60.0),
            ghost_speed_mul: 1.1,
            difficulty_rate: 0.28,
            spawn_rate_mul: 1.2,
            ghost_flicker: false,
            ghost_invisible_far: false,
        },
        GameMode::Nightmare => ModeConfig {
            time_limit: None,
            ghost_speed_mul: 1.25,
            difficulty_rate: 0.32,
            spawn_rate_mul: 1.35,
            ghost_flicker: true,
            ghost_invisible_far: true,
        },
    }
}

#[derive(Clone, Copy)]
struct Settings {
    audio_enabled: bool,
    master_volume: f32,
    shake_enabled: bool,
    vignette: f32, // 0..1
    fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_enabled: true,
            master_volume: 0.6,
            shake_enabled: true,
            vignette: 0.6,
            fullscreen: false,
        }
    }
}

enum Scene {
    MainMenu { selected: usize },
    Settings { selected: usize },
    Playing,
    GameOver { best: f32, score: f32 },
}

struct World {
    player: Player,
    ghosts: Vec<Ghost>,
    orbs: Vec<Orb>,

    // History buffer for ghosts
    history: VecDeque<InputFrame>,
    history_max: usize,

    // Timers and progression
    time_alive: f32,
    score: f32,
    combo: f32,
    last_collect_time: f32,

    orb_spawn_timer: f32,

    // Difficulty dial
    difficulty: f32,

    // Camera shake
    shake_t: f32,
    shake_amt: f32,

    // Audio
    audio: AudioSet,

    // Meta
    mode: GameMode,
    config: ModeConfig,
    settings: Settings,
}

impl World {
    fn difficulty_spawn_interval(&self) -> f32 {
        let mut s = ORB_SPAWN_BASE * (1.0 / (1.0 + 0.25 * self.difficulty)) / self.config.spawn_rate_mul;
        s = s.max(ORB_SPAWN_MIN);
        s
    }

    fn ghost_speed(&self) -> f32 {
        // 1.0 means 60 samples/sec. Scale gently
        (1.0 + 0.3 * self.difficulty) * self.config.ghost_speed_mul
    }

    fn ghost_ttl(&self) -> f32 {
        // Longer lasting ghosts as difficulty increases, but cap it
        (8.0 + self.difficulty * 2.0).min(18.0)
    }

    fn spawn_ghost(&mut self, recent_secs: f32) {
        let frames_recent = (recent_secs / FIXED_DT) as usize;
        if self.history.len() < frames_recent.saturating_add(10) {
            return; // not enough data yet
        }
        let start = self.history.len() - frames_recent;
        let samples: Vec<Vec2> = self.history.iter().skip(start).map(|f| f.pos).collect();
        if samples.len() < 12 {
            return;
        }
        let ghost = Ghost {
            samples,
            progress: 0.0,
            speed: self.ghost_speed(),
            radius: GHOST_RADIUS,
            color: Color::new(1.0, 0.35, 0.35, 0.9),
            ttl: self.ghost_ttl(),
        };
        self.ghosts.push(ghost);
        if self.settings.audio_enabled {
            play_sound(
                &self.audio.ghost_spawn,
                PlaySoundParams {
                    looped: false,
                    volume: 0.55 * self.settings.master_volume,
                },
            );
        }
    }

    fn spawn_orb(&mut self, w: f32, h: f32) {
        let o = Orb {
            pos: rand_pos_away_from(self.player.pos, ORB_SAFE_RADIUS, w, h),
            radius: ORB_RADIUS,
            alive: true,
        };
        self.orbs.push(o);
    }

    fn add_shake(&mut self, power: f32, time: f32) {
        if self.settings.shake_enabled {
            self.shake_amt = self.shake_amt.max(power);
            self.shake_t = self.shake_t.max(time);
        }
    }

    fn camera_offset(&self) -> Vec2 {
        if self.shake_t <= 0.0 {
            return vec2(0.0, 0.0);
        }
        use macroquad::rand::gen_range;
        vec2(
            gen_range(-1.0, 1.0) * self.shake_amt,
            gen_range(-1.0, 1.0) * self.shake_amt,
        )
    }
}

// -------------------------------
// Main Loop
// -------------------------------
#[macroquad::main(window_conf)]
async fn main() {
    // Preload sounds
    let sfx_collect = load_sound_from_bytes(&tone_wav(880.0, 0.12, 0.45, 0.002, 0.02))
        .await
        .unwrap();
    let sfx_ghost = load_sound_from_bytes(&tone_wav(420.0, 0.18, 0.35, 0.004, 0.03))
        .await
        .unwrap();
    let sfx_death = load_sound_from_bytes(&tone_wav(120.0, 0.45, 0.5, 0.0, 0.06))
        .await
        .unwrap();
    let sfx_drone = load_sound_from_bytes(&tone_wav(55.0, 1.5, 0.25, 0.01, 0.1))
        .await
        .unwrap();

    let audio = AudioSet {
        collect: sfx_collect,
        ghost_spawn: sfx_ghost,
        death: sfx_death,
        drone: sfx_drone,
    };

    let mut settings = Settings::default();
    let mut mode = GameMode::Classic;
    let mut best_scores = [0.0f32; 3];
    let mut scene = Scene::MainMenu { selected: 0 };

    loop {
        clear_background(BLACK);

        match scene {
            Scene::MainMenu { ref mut selected } => {
                draw_main_menu(*selected, mode, &settings, &best_scores);
                if let Some(action) = update_main_menu(selected, &mut mode) {
                    match action {
                        MainMenuAction::Start => scene = Scene::Playing,
                        MainMenuAction::Settings => scene = Scene::Settings { selected: 0 },
                        MainMenuAction::Quit => std::process::exit(0),
                    }
                }
                if is_key_pressed(KeyCode::F11) {
                    settings.fullscreen = !settings.fullscreen;
                    set_fullscreen(settings.fullscreen);
                }
            }
            Scene::Settings { ref mut selected } => {
                draw_settings_menu(*selected, &settings);
                if update_settings_menu(selected, &mut settings) {
                    scene = Scene::MainMenu { selected: 0 };
                }
            }
            Scene::Playing => {
                let mut world = new_world(audio.clone(), settings, mode);
                let mut acc = 0.0f32;

                'game: loop {
                    let dt = get_frame_time() as f32;
                    acc += dt;

                    while acc >= FIXED_DT {
                        if step(&mut world) {
                            // game over
                            break 'game;
                        }
                        acc -= FIXED_DT;
                    }

                    draw_world(&world);
                    next_frame().await;
                }

                if world.settings.audio_enabled {
                    play_sound(
                        &world.audio.death,
                        PlaySoundParams { looped: false, volume: 0.7 * world.settings.master_volume },
                    );
                    stop_sound(&world.audio.drone);
                }

                let idx = world.mode.index();
                best_scores[idx] = best_scores[idx].max(world.score);
                scene = Scene::GameOver { best: best_scores[idx], score: world.score };
            }
            Scene::GameOver { best, score } => {
                draw_game_over(score, best);
                if is_key_pressed(KeyCode::Enter) {
                    scene = Scene::Playing;
                } else if is_key_pressed(KeyCode::Escape) {
                    scene = Scene::MainMenu { selected: 0 };
                }
                if is_key_pressed(KeyCode::F11) {
                    settings.fullscreen = !settings.fullscreen;
                    set_fullscreen(settings.fullscreen);
                }
            }
        }

        next_frame().await;
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Feedback Rush".to_string(),
        window_width: WIDTH as i32,
        window_height: HEIGHT as i32,
        high_dpi: true,
        fullscreen: false,
        ..Default::default()
    }
}

// -------------------------------
// World creation
// -------------------------------
fn new_world(audio: AudioSet, settings: Settings, mode: GameMode) -> World {
    let history_max = (INPUT_HISTORY_SECONDS / FIXED_DT) as usize;
    let config = mode_config(mode);

    let mut w = World {
        player: Player::new(vec2(screen_width() * 0.5, screen_height() * 0.5)),
        ghosts: Vec::new(),
        orbs: Vec::new(),

        history: VecDeque::with_capacity(history_max + 1),
        history_max,

        time_alive: 0.0,
        score: 0.0,
        combo: 1.0,
        last_collect_time: -999.0,

        orb_spawn_timer: 0.0,

        difficulty: 0.0,

        shake_t: 0.0,
        shake_amt: 0.0,

        audio,
        mode,
        config,
        settings,
    };

    if w.settings.audio_enabled {
        play_sound(
            &w.audio.drone,
            PlaySoundParams {
                looped: true,
                volume: 0.15 * w.settings.master_volume,
            },
        );
    }

    w
}

// -------------------------------
// One fixed-timestep step
// Returns true on game over
// -------------------------------
fn step(w: &mut World) -> bool {
    let sw = screen_width();
    let sh = screen_height();
    w.time_alive += FIXED_DT;
    w.difficulty = w.config.difficulty_rate * w.time_alive + 0.002 * w.score; // mode ramp

    if let Some(limit) = w.config.time_limit {
        if w.time_alive >= limit {
            return true;
        }
    }

    // Spawn orbs over time
    w.orb_spawn_timer -= FIXED_DT;
    if w.orb_spawn_timer <= 0.0 {
        w.spawn_orb(sw, sh);
        w.orb_spawn_timer = w.difficulty_spawn_interval();
    }

    // Read input
    let mut dir = vec2(0.0, 0.0);
    if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
        dir.x -= 1.0;
    }
    if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
        dir.x += 1.0;
    }
    if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
        dir.y -= 1.0;
    }
    if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
        dir.y += 1.0;
    }
    if dir.length_squared() > 1.0 {
        dir = dir.normalize();
    }

    // Phase ability
    let want_phase =
        is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) || is_key_down(KeyCode::Space);
    if want_phase && w.player.phase_energy > 0.0 {
        w.player.phase_active = true;
        w.player.phase_energy -= PHASE_DRAIN * FIXED_DT;
        if w.player.phase_energy <= 0.0 {
            w.player.phase_energy = 0.0;
            w.player.phase_active = false;
        }
    } else {
        w.player.phase_active = false;
        w.player.phase_energy = (w.player.phase_energy + PHASE_REGEN * FIXED_DT).min(PHASE_MAX);
    }

    // Physics
    w.player.vel += dir * ACCEL * FIXED_DT;
    if w.player.vel.length() > MAX_SPEED {
        w.player.vel = w.player.vel.normalize() * MAX_SPEED;
    }
    // Friction
    w.player.vel *= (1.0 - FRICTION * FIXED_DT).max(0.0);
    w.player.pos += w.player.vel * FIXED_DT;

    // Bounds clamp
    let (clamped, _n) = clamp_rect(w.player.pos, w.player.radius, sw, sh);
    w.player.pos = clamped;

    // Push to history
    w.history.push_back(InputFrame { pos: w.player.pos });
    while w.history.len() > w.history_max {
        w.history.pop_front();
    }

    // Update ghosts
    for g in &mut w.ghosts {
        g.ttl -= FIXED_DT;
        g.progress += g.speed * FIXED_DT * 60.0; // samples/sec scaled vs 60fps recording
    }
    w.ghosts.retain(|g| g.ttl > 0.0);

    // Collisions with ghosts
    if !w.player.phase_active {
        for g in &w.ghosts {
            if circle_overlap(w.player.pos, w.player.radius, g.current_pos(), g.radius) {
                // game over
                w.add_shake(8.0, 0.25);
                return true;
            }
        }
    }

    // Collide with orbs
    let mut collected_count = 0u32;
    for o in &mut w.orbs {
        if o.alive && circle_overlap(w.player.pos, w.player.radius, o.pos, o.radius) {
            o.alive = false;
            collected_count += 1;

            // Score and combo
            let since = (w.time_alive - w.last_collect_time).max(0.0);
            if since < 1.6 {
                w.combo += 0.25;
            } else {
                w.combo = (w.combo - COMBO_DECAY_PER_SEC * since).max(1.0);
                w.combo += 0.15;
            }
            w.last_collect_time = w.time_alive;

            let gain = 45.0 * w.combo;
            w.score += gain;
        }
    }
    if collected_count > 0 {
        // Spawn ghosts: replay last 2.6..5.0s depending on difficulty
        let secs = lerp(2.6, 5.0, (w.difficulty / 12.0).min(1.0));
        for _ in 0..collected_count {
            w.spawn_ghost(secs);
        }
        // SFX + shake
        if w.settings.audio_enabled {
            play_sound(
                &w.audio.collect,
                PlaySoundParams {
                    looped: false,
                    volume: 0.55 * w.settings.master_volume,
                },
            );
        }
        w.add_shake(3.0, 0.12);
    }
    w.orbs.retain(|o| o.alive);

    // Passive score over time with combo influence that decays slowly
    let decay = COMBO_DECAY_PER_SEC * FIXED_DT;
    w.combo = (w.combo - decay).max(1.0);
    w.score += (2.0 + w.difficulty * 0.4) * FIXED_DT * w.combo;

    // Camera shake timer
    if w.shake_t > 0.0 {
        w.shake_t -= FIXED_DT;
        if w.shake_t <= 0.0 {
            w.shake_t = 0.0;
            w.shake_amt = 0.0;
        }
    }

    false
}

// -------------------------------
// Rendering
// -------------------------------
fn draw_world(w: &World) {
    let sw = screen_width();
    let sh = screen_height();
    let cam_off = w.camera_offset();

    // Arena background
    let bg = Color::new(0.06, 0.07, 0.10, 1.0);
    draw_rectangle(0.0 + cam_off.x, 0.0 + cam_off.y, sw, sh, bg);

    // Faint grid
    let grid_c = Color::new(0.12, 0.13, 0.17, 1.0);
    for x in (0..sw as i32).step_by(40) {
        draw_line(
            x as f32 + cam_off.x,
            0.0 + cam_off.y,
            x as f32 + cam_off.x,
            sh + cam_off.y,
            1.0,
            grid_c,
        );
    }
    for y in (0..sh as i32).step_by(40) {
        draw_line(
            0.0 + cam_off.x,
            y as f32 + cam_off.y,
            sw + cam_off.x,
            y as f32 + cam_off.y,
            1.0,
            grid_c,
        );
    }

    // Orbs
    for o in &w.orbs {
        draw_circle(o.pos.x + cam_off.x, o.pos.y + cam_off.y, o.radius, YELLOW);
        draw_circle_lines(
            o.pos.x + cam_off.x,
            o.pos.y + cam_off.y,
            o.radius + 3.0,
            2.0,
            Color::new(0.9, 0.8, 0.2, 0.5),
        );
    }

    // Ghosts, draw path hints sparsely and current position
    for g in &w.ghosts {
        let mut alpha = (g.ttl / (g.ttl + 1.0)).clamp(0.25, 0.9);
        if w.config.ghost_flicker {
            let flick = (w.time_alive * 7.0 + g.progress * 0.05).sin().abs();
            alpha *= 0.4 + 0.6 * flick;
        }
        let pos = g.current_pos();
        if w.config.ghost_invisible_far {
            let dist = pos.distance(w.player.pos);
            if dist > 220.0 { alpha *= 0.25; }
        }
        let c = Color::new(0.95, 0.25, 0.25, alpha);
        draw_circle(pos.x + cam_off.x, pos.y + cam_off.y, g.radius, c);

        // Sparse path dots
        let step = (g.samples.len() / 24).max(4);
        for (i, s) in g.samples.iter().enumerate().step_by(step) {
            let _ = i; // silence unused warning when step is large
            draw_circle(s.x + cam_off.x, s.y + cam_off.y, 2.0, Color::new(0.8, 0.2, 0.2, 0.18));
        }
    }

    // Player
    let pc = if w.player.phase_active {
        Color::new(0.45, 0.9, 0.95, 1.0)
    } else {
        Color::new(0.35, 0.75, 1.0, 1.0)
    };
    draw_circle(
        w.player.pos.x + cam_off.x,
        w.player.pos.y + cam_off.y,
        w.player.radius,
        pc,
    );
    draw_circle_lines(
        w.player.pos.x + cam_off.x,
        w.player.pos.y + cam_off.y,
        w.player.radius + 4.0,
        2.0,
        Color::new(0.2, 0.45, 0.9, 0.65),
    );

    // UI
    draw_ui(w);

    // Horror vignette overlay
    draw_vignette(sw, sh, w.settings.vignette, w.difficulty, w.config.ghost_flicker);
}

fn draw_ui(w: &World) {
    let sw = screen_width();
    let s = format!(
        "Score: {:>6}   x{:.2}   Time: {:>5.1}s",
        w.score as i32, w.combo, w.time_alive
    );
    draw_text(&s, 16.0, 28.0, 26.0, WHITE);

    // Phase bar
    let bar_w = 200.0;
    let bar_h = 12.0;
    let x = 16.0;
    let y = 40.0;
    draw_rectangle_lines(x - 2.0, y - 2.0, bar_w + 4.0, bar_h + 4.0, 2.0, GRAY);
    let t = (w.player.phase_energy / PHASE_MAX).clamp(0.0, 1.0);
    draw_rectangle(x, y, bar_w * t, bar_h, Color::new(0.25, 0.9, 0.95, 0.9));

    // Difficulty indicator
    let d = format!("Threat: {:.1}", w.difficulty);
    let dims = measure_text(&d, None, 26, 1.0);
    draw_text(
        &d,
        sw - dims.width - 16.0,
        28.0,
        26.0,
        Color::new(0.9, 0.5, 0.5, 1.0),
    );

    // Mode label
    let ml = format!("Mode: {}", w.mode.name());
    draw_text(&ml, 16.0, 64.0, 22.0, GRAY);
}

fn draw_title_screen(best: f32) {
    let sw = screen_width();
    let sh = screen_height();
    clear_background(BLACK);
    // Title centered
    let title = "Feedback Rush";
    let td = measure_text(title, None, 64, 1.0);
    draw_text(title, (sw - td.width) * 0.5, 120.0, 64.0, WHITE);
    let subt = "Outmaneuver your own echoes.";
    let sd = measure_text(subt, None, 28, 1.0);
    draw_text(subt, (sw - sd.width) * 0.5, 160.0, 28.0, GRAY);

    let controls = [
        "WASD / Arrows - Move",
        "Shift or Space - Phase (invulnerable, drains meter)",
        "Collect orbs to score and spawn 'ghost' echoes",
        "Avoid colliding with ghosts unless phasing",
        "Your ghosts loop your past path at increasing speed",
    ];
    let mut y = 220.0;
    for c in controls {
        let cd = measure_text(c, None, 24, 1.0);
        draw_text(c, (sw - cd.width) * 0.5, y, 24.0, LIGHTGRAY);
        y += 28.0;
    }

    let prompt = "Press Enter to start";
    let pd = measure_text(prompt, None, 28, 1.0);
    draw_text(prompt, (sw - pd.width) * 0.5, sh - 64.0, 28.0, Color::new(0.8, 0.9, 1.0, 1.0));

    if best > 0.0 {
        let btxt = format!("Best Score: {}", best as i32);
        let bd = measure_text(&btxt, None, 28, 1.0);
        draw_text(&btxt, sw - bd.width - 24.0, sh - 36.0, 28.0, WHITE);
    }
}

fn draw_game_over(score: f32, best: f32) {
    let sw = screen_width();
    let sh = screen_height();
    clear_background(Color::new(0.05, 0.05, 0.06, 1.0));
    let t = "Run Over";
    let td = measure_text(t, None, 64, 1.0);
    draw_text(t, (sw - td.width) * 0.5, 120.0, 64.0, Color::new(1.0, 0.5, 0.5, 1.0));

    let s1 = format!("Score: {}", score as i32);
    let s1d = measure_text(&s1, None, 32, 1.0);
    draw_text(&s1, (sw - s1d.width) * 0.5, 170.0, 32.0, WHITE);

    let s2 = format!("Best:  {}", best as i32);
    let s2d = measure_text(&s2, None, 32, 1.0);
    draw_text(&s2, (sw - s2d.width) * 0.5, 206.0, 32.0, WHITE);

    let p = "Enter - Restart / Esc - Menu";
    let pd = measure_text(p, None, 28, 1.0);
    draw_text(p, (sw - pd.width) * 0.5, sh - 64.0, 28.0, GRAY);
}

fn draw_vignette(sw: f32, sh: f32, strength: f32, threat: f32, pulse: bool) {
    if strength <= 0.01 {
        return;
    }
    let center = vec2(sw * 0.5, sh * 0.5);
    let max_r = center.length().max(sw.max(sh));
    let rings = 14;
    let base_alpha = 0.08 * strength;
    let mut alpha_boost = 0.0;
    if pulse {
        let t = get_time() as f32;
        let beat = (t * (1.0 + threat * 0.2)).sin().max(0.0);
        alpha_boost = 0.06 * beat * strength;
    }
    for i in 0..rings {
        let k = i as f32 / rings as f32;
        let r = lerp(max_r * 0.55, max_r * 0.95, k);
        let a = base_alpha * (1.0 - k) + alpha_boost * (1.0 - k);
        draw_circle_lines(center.x, center.y, r, 8.0, Color::new(0.0, 0.0, 0.0, a));
    }
}

fn draw_main_menu(selected: usize, mode: GameMode, settings: &Settings, bests: &[f32; 3]) {
    let sw = screen_width();
    let sh = screen_height();
    clear_background(BLACK);
    // Title centered
    let title = "Feedback Rush";
    let td = measure_text(title, None, 64, 1.0);
    draw_text(title, (sw - td.width) * 0.5, 110.0, 64.0, WHITE);
    let subt = "Outmaneuver your own echoes.";
    let sd = measure_text(subt, None, 24, 1.0);
    draw_text(subt, (sw - sd.width) * 0.5, 150.0, 24.0, GRAY);

    let items = [
        "Start Game",
        &format!("Mode: {}", mode.name()),
        "Settings",
        "Quit",
    ];
    let mut y = 220.0;
    for (i, txt) in items.iter().enumerate() {
        let c = if i == selected { Color::new(0.9, 0.9, 1.0, 1.0) } else { LIGHTGRAY };
        let size = if i == selected { 30.0 } else { 26.0 };
        let md = measure_text(txt, None, size as u16, 1.0);
        draw_text(txt, (sw - md.width) * 0.5, y, size, c);
        y += 36.0;
    }

    let best = bests[mode.index()] as i32;
    let btxt = format!("Best {}: {}", mode.name(), best);
    let bd = measure_text(&btxt, None, 22, 1.0);
    draw_text(&btxt, (sw - bd.width) * 0.5, y + 16.0, 22.0, GRAY);

    let hint = "Enter: Select  |  Arrows: Navigate  |  F11: Fullscreen";
    let hd = measure_text(hint, None, 20, 1.0);
    draw_text(hint, (sw - hd.width) * 0.5, sh - 40.0, 20.0, DARKGRAY);

    draw_vignette(sw, sh, settings.vignette, 0.0, false);
}

enum MainMenuAction { Start, Settings, Quit }

fn update_main_menu(selected: &mut usize, mode: &mut GameMode) -> Option<MainMenuAction> {
    let count = 4usize;
    if is_key_pressed(KeyCode::Up) {
        if *selected == 0 { *selected = count - 1; } else { *selected -= 1; }
    }
    if is_key_pressed(KeyCode::Down) {
        *selected = (*selected + 1) % count;
    }
    if is_key_pressed(KeyCode::Left) {
        if *selected == 1 {
            let idx = (mode.index() + 2) % 3; // prev
            *mode = GameMode::from_index(idx);
        }
    }
    if is_key_pressed(KeyCode::Right) {
        if *selected == 1 {
            let idx = (mode.index() + 1) % 3; // next
            *mode = GameMode::from_index(idx);
        }
    }
    if is_key_pressed(KeyCode::Enter) {
        return Some(match *selected {
            0 => MainMenuAction::Start,
            1 => return None,
            2 => MainMenuAction::Settings,
            3 => MainMenuAction::Quit,
            _ => return None,
        });
    }
    None
}

fn draw_settings_menu(selected: usize, s: &Settings) {
    let sw = screen_width();
    let sh = screen_height();
    clear_background(BLACK);
    let title = "Settings";
    let td = measure_text(title, None, 56, 1.0);
    draw_text(title, (sw - td.width) * 0.5, 110.0, 56.0, WHITE);

    let items = [
        format!("Audio: {}", if s.audio_enabled { "On" } else { "Off" }),
        format!("Volume: {:.0}%", (s.master_volume * 100.0).round()),
        format!("Shake: {}", if s.shake_enabled { "On" } else { "Off" }),
        format!("Vignette: {:.0}%", (s.vignette * 100.0).round()),
        format!("Fullscreen: {}", if s.fullscreen { "On" } else { "Off" }),
        "Back".to_string(),
    ];

    let mut y = 220.0;
    for (i, txt) in items.iter().enumerate() {
        let c = if i == selected { Color::new(0.9, 0.9, 1.0, 1.0) } else { LIGHTGRAY };
        let size = if i == selected { 28.0 } else { 24.0 };
        let md = measure_text(txt, None, size as u16, 1.0);
        draw_text(txt, (sw - md.width) * 0.5, y, size, c);
        y += 32.0;
    }

    let hint = "Enter/Left/Right to change, Esc to back, F11 Fullscreen";
    let hd = measure_text(hint, None, 20, 1.0);
    draw_text(hint, (sw - hd.width) * 0.5, sh - 40.0, 20.0, DARKGRAY);

    draw_vignette(sw, sh, s.vignette, 0.0, false);
}

fn update_settings_menu(selected: &mut usize, s: &mut Settings) -> bool {
    let count = 6usize;
    if is_key_pressed(KeyCode::Up) {
        if *selected == 0 { *selected = count - 1; } else { *selected -= 1; }
    }
    if is_key_pressed(KeyCode::Down) {
        *selected = (*selected + 1) % count;
    }
    if is_key_pressed(KeyCode::Left) {
        match *selected {
            0 => s.audio_enabled = !s.audio_enabled,
            1 => s.master_volume = (s.master_volume - 0.1).clamp(0.0, 1.0),
            2 => s.shake_enabled = !s.shake_enabled,
            3 => s.vignette = (s.vignette - 0.1).clamp(0.0, 1.0),
            4 => { s.fullscreen = !s.fullscreen; set_fullscreen(s.fullscreen); },
            _ => {}
        }
    }
    if is_key_pressed(KeyCode::Right) {
        match *selected {
            0 => s.audio_enabled = !s.audio_enabled,
            1 => s.master_volume = (s.master_volume + 0.1).clamp(0.0, 1.0),
            2 => s.shake_enabled = !s.shake_enabled,
            3 => s.vignette = (s.vignette + 0.1).clamp(0.0, 1.0),
            4 => { s.fullscreen = !s.fullscreen; set_fullscreen(s.fullscreen); },
            _ => {}
        }
    }
    if is_key_pressed(KeyCode::Enter) {
        if *selected == 5 { return true; }
    }
    if is_key_pressed(KeyCode::Escape) {
        return true;
    }
    if is_key_pressed(KeyCode::F11) {
        s.fullscreen = !s.fullscreen;
        set_fullscreen(s.fullscreen);
    }
    false
}
