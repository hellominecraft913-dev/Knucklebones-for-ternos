#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::Text,
};

// 1. Bare-metal Panic Handler
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// Stack string writer for embedded/no_std memory environments
pub struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> BufWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}

impl<'a> Write for BufWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        if bytes.len() > remaining {
            return Err(core::fmt::Error);
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Button {
    Left,
    Right,
    Select,
    Back,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum GameState {
    Menu,
    PlayerRoll,
    PlayerPlace,
    GameOver,
}

pub struct KnucklebonesApp {
    pub player_grid: [[u8; 3]; 3],
    pub player_counts: [usize; 3],
    pub ai_grid: [[u8; 3]; 3],
    pub ai_counts: [usize; 3],
    pub state: GameState,
    pub current_die: u8,
    pub selected_col: usize,
    pub msg: &'static str,
    rng_seed: u32,
}

impl KnucklebonesApp {
    pub fn new() -> Self {
        let mut app = Self {
            player_grid: [[0; 3]; 3],
            player_counts: [0; 3],
            ai_grid: [[0; 3]; 3],
            ai_counts: [0; 3],
            state: GameState::Menu,
            current_die: 0,
            selected_col: 1,
            msg: "PRESS OK TO START",
            rng_seed: 0x12345678,
        };
        app.reset();
        app
    }

    pub fn reset(&mut self) {
        self.player_grid = [[0; 3]; 3];
        self.player_counts = [0; 3];
        self.ai_grid = [[0; 3]; 3];
        self.ai_counts = [0; 3];
        self.state = GameState::Menu;
        self.current_die = 0;
        self.selected_col = 1;
        self.msg = "PRESS OK TO START";
    }

    fn rand_die(&mut self) -> u8 {
        self.rng_seed = self.rng_seed.wrapping_mul(1103515245).wrapping_add(12345);
        (((self.rng_seed / 65536) % 6) + 1) as u8
    }

    pub fn col_score(col: &[u8], count: usize) -> u32 {
        let mut counts = [0u32; 7];
        for i in 0..count {
            let v = col[i] as usize;
            if (1..=6).contains(&v) {
                counts[v] += 1;
            }
        }
        let mut sum = 0;
        for v in 1..=6 {
            if counts[v] > 0 {
                sum += (v as u32) * counts[v] * counts[v];
            }
        }
        sum
    }

    pub fn total_score(grid: &[[u8; 3]; 3], counts: &[usize; 3]) -> u32 {
        (0..3).map(|c| Self::col_score(&grid[c], counts[c])).sum()
    }

    pub fn is_grid_full(counts: &[usize; 3]) -> bool {
        counts[0] >= 3 && counts[1] >= 3 && counts[2] >= 3
    }

    fn place_die(&mut self, is_player: bool, col: usize, val: u8) -> usize {
        let (grid, counts, opp_grid, opp_counts) = if is_player {
            (
                &mut self.player_grid,
                &mut self.player_counts,
                &mut self.ai_grid,
                &mut self.ai_counts,
            )
        } else {
            (
                &mut self.ai_grid,
                &mut self.ai_counts,
                &mut self.player_grid,
                &mut self.player_counts,
            )
        };

        if counts[col] < 3 {
            grid[col][counts[col]] = val;
            counts[col] += 1;
        }

        let mut destroyed = 0;
        let mut new_opp = [0u8; 3];
        let mut new_opp_count = 0;

        for i in 0..opp_counts[col] {
            if opp_grid[col][i] == val {
                destroyed += 1;
            } else {
                new_opp[new_opp_count] = opp_grid[col][i];
                new_opp_count += 1;
            }
        }

        for i in 0..new_opp_count {
            opp_grid[col][i] = new_opp[i];
        }
        opp_counts[col] = new_opp_count;

        destroyed
    }

    fn move_cursor(&mut self, dir: i32) {
        let mut c = self.selected_col as i32;
        for _ in 0..3 {
            c += dir;
            if c < 0 {
                c = 2;
            }
            if c > 2 {
                c = 0;
            }
            if self.player_counts[c as usize] < 3 {
                self.selected_col = c as usize;
                return;
            }
        }
    }

    fn ai_choose_column(&mut self, val: u8) -> usize {
        let mut best_col = 0;
        let mut best_eval = -999999i32;

        for c in 0..3 {
            if self.ai_counts[c] < 3 {
                let old_ai = Self::col_score(&self.ai_grid[c], self.ai_counts[c]);
                self.ai_grid[c][self.ai_counts[c]] = val;
                let new_ai = Self::col_score(&self.ai_grid[c], self.ai_counts[c] + 1);
                let gain = (new_ai - old_ai) as i32;

                let old_p = Self::col_score(&self.player_grid[c], self.player_counts[c]);

                let mut temp_p_len = 0;
                for i in 0..self.player_counts[c] {
                    if self.player_grid[c][i] != val {
                        temp_p_len += 1;
                    }
                }

                let destroyed = self.player_counts[c] - temp_p_len;
                let loss = (old_p - Self::col_score(&self.player_grid[c], temp_p_len)) as i32;

                let eval = gain + loss + (destroyed as i32 * val as i32 * 2);
                if eval > best_eval {
                    best_eval = eval;
                    best_col = c;
                }
            }
        }
        best_col
    }

    pub fn handle_button(&mut self, btn: Button) {
        match btn {
            Button::Back => self.reset(),
            Button::Left => {
                if self.state == GameState::PlayerPlace {
                    self.move_cursor(-1);
                }
            }
            Button::Right => {
                if self.state == GameState::PlayerPlace {
                    self.move_cursor(1);
                }
            }
            Button::Select => match self.state {
                GameState::Menu | GameState::GameOver => {
                    self.reset();
                    self.state = GameState::PlayerRoll;
                    self.msg = "YOUR TURN! [ROLL]";
                }
                GameState::PlayerRoll => {
                    self.current_die = self.rand_die();
                    self.state = GameState::PlayerPlace;
                    self.msg = "PLACE YOUR DIE";
                    if self.player_counts[self.selected_col] >= 3 {
                        self.move_cursor(1);
                    }
                }
                GameState::PlayerPlace => {
                    if self.player_counts[self.selected_col] < 3 {
                        self.place_die(true, self.selected_col, self.current_die);

                        if Self::is_grid_full(&self.player_counts)
                            || Self::is_grid_full(&self.ai_counts)
                        {
                            self.state = GameState::GameOver;
                        } else {
                            let ai_roll = self.rand_die();
                            let ai_col = self.ai_choose_column(ai_roll);
                            self.place_die(false, ai_col, ai_roll);

                            if Self::is_grid_full(&self.player_counts)
                                || Self::is_grid_full(&self.ai_counts)
                            {
                                self.state = GameState::GameOver;
                            } else {
                                self.state = GameState::PlayerRoll;
                                self.msg = "YOUR TURN!";
                            }
                        }
                    }
                }
            },
        }
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        target.clear(BinaryColor::Off)?;

        let text_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let stroke_normal = PrimitiveStyleBuilder::new()
            .stroke_color(BinaryColor::On)
            .stroke_width(2)
            .build();
        let stroke_thick = PrimitiveStyleBuilder::new()
            .stroke_color(BinaryColor::On)
            .stroke_width(5)
            .build();

        if self.state == GameState::Menu {
            Text::new("KNUCKLEBONES", Point::new(180, 320), text_style).draw(target)?;
            Text::new("TernOS Edition", Point::new(170, 360), text_style).draw(target)?;
            Text::new("[ PRESS OK ]", Point::new(180, 440), text_style).draw(target)?;
            return Ok(());
        }

        let p_score = Self::total_score(&self.player_grid, &self.player_counts);
        let ai_score = Self::total_score(&self.ai_grid, &self.ai_counts);

        // Header (AI Score)
        let mut score_buf = [0u8; 32];
        let mut w = BufWriter::new(&mut score_buf);
        let _ = core::fmt::write(&mut w, format_args!("OPPONENT (AI): {}", ai_score));
        Text::new(w.as_str(), Point::new(25, 35), text_style).draw(target)?;

        // AI Grid
        for c in 0..3 {
            let col_x = 25 + (c as i32) * 155;
            Rectangle::new(Point::new(col_x, 50), Size::new(120, 210))
                .into_styled(stroke_normal)
                .draw(target)?;

            for s in 0..3 {
                if s < self.ai_counts[c] {
                    let val = self.ai_grid[c][s];
                    let mut val_buf = [0u8; 8];
                    let mut vw = BufWriter::new(&mut val_buf);
                    let _ = core::fmt::write(&mut vw, format_args!("{}", val));
                    Text::new(
                        vw.as_str(),
                        Point::new(col_x + 55, 95 + (s as i32) * 60),
                        text_style,
                    )
                    .draw(target)?;
                }
            }
        }

        // Status & Rolled Die
        Text::new(self.msg, Point::new(140, 320), text_style).draw(target)?;
        if self.current_die > 0 {
            let mut die_buf = [0u8; 16];
            let mut dw = BufWriter::new(&mut die_buf);
            let _ = core::fmt::write(&mut dw, format_args!("DIE: [ {} ]", self.current_die));
            Text::new(dw.as_str(), Point::new(180, 370), text_style).draw(target)?;
        }

        // Player Grid
        for c in 0..3 {
            let col_x = 25 + (c as i32) * 155;
            let is_sel = self.state == GameState::PlayerPlace && self.selected_col == c;

            Rectangle::new(Point::new(col_x, 440), Size::new(120, 210))
                .into_styled(if is_sel { stroke_thick } else { stroke_normal })
                .draw(target)?;

            for s in 0..3 {
                if s < self.player_counts[c] {
                    let val = self.player_grid[c][s];
                    let mut val_buf = [0u8; 8];
                    let mut vw = BufWriter::new(&mut val_buf);
                    let _ = core::fmt::write(&mut vw, format_args!("{}", val));
                    Text::new(
                        vw.as_str(),
                        Point::new(col_x + 55, 485 + (s as i32) * 60),
                        text_style,
                    )
                    .draw(target)?;
                }
            }
        }

        // Footer (Player Score)
        let mut p_score_buf = [0u8; 32];
        let mut pw = BufWriter::new(&mut p_score_buf);
        let _ = core::fmt::write(&mut pw, format_args!("PLAYER SCORE: {}", p_score));
        Text::new(pw.as_str(), Point::new(25, 695), text_style).draw(target)?;

        if self.state == GameState::GameOver {
            let outcome = if p_score > ai_score {
                "YOU WON!"
            } else if ai_score > p_score {
                "AI WON!"
            } else {
                "IT'S A TIE!"
            };
            Text::new(outcome, Point::new(180, 740), text_style).draw(target)?;
        }

        Ok(())
    }
}

// 2. Exported Entry Point Symbol for Bare-metal RISC-V Runtime
#[no_mangle]
pub extern "C" fn main() -> ! {
    let mut _app = KnucklebonesApp::new();

    loop {
        // Main loop on hardware
    }
}