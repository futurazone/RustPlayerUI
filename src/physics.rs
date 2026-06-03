//! Física del carrusel y scroll vertical (portado de la implementación Python).
//!
//! - `SwiperPhysics`: carrusel horizontal con spring-damper para snapping.
//!   El reciclaje de slots (lib_offset) se gestiona en el timer tick de main.
//! - `VerticalPhysics`: scroll del TrackPicker con fricción y bounce elástico
//!   cuando se sale de los límites (min_offset / max_offset).

pub struct SwiperPhysics {
    pub offset_x: f32,
    pub velocity: f32,
    pub snap_target: f32,

    // Config values (from Python)
    pub spring_k: f32,
    pub spring_c: f32,
    pub max_velocity: f32,
    pub min_velocity: f32,
    pub is_moving: bool,
    pub spacing: f32,

    // Recycling state
    pub lib_offset: i32,
}

use crate::config::CENTER_INDEX;

impl SwiperPhysics {
    pub fn new() -> Self {
        Self {
            offset_x: 0.0,
            velocity: 0.0,
            snap_target: 0.0,
            spring_k: 36.0,
            spring_c: 11.0,
            max_velocity: 5200.0,
            min_velocity: 12.0,
            is_moving: false,
            spacing: 350.0,
            lib_offset: -CENTER_INDEX,
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
        if !self.is_moving {
            return false;
        }

        let dist = self.snap_target - self.offset_x;

        if dist.abs() < 0.5 && self.velocity.abs() < self.min_velocity {
            self.offset_x = self.snap_target;
            self.snap_target = 0.0;
            self.velocity = 0.0;
            self.is_moving = false;
            return true;
        }

        let accel = (self.spring_k * dist) - (self.spring_c * self.velocity);
        self.velocity += accel * dt;
        self.velocity = self.velocity.clamp(-self.max_velocity, self.max_velocity);

        self.offset_x += self.velocity * dt;

        // No llamamos a check_recycling aquí para que el bucle de main.rs
        // pueda detectar el cambio y actualizar los modelos de Slint sincrónicamente.

        true
    }

    pub fn set_snap_slot(&mut self, dx: f32, velocity: f32) {
        let displacement_slots = dx / self.spacing;
        // Tuning simplificado: sin histéresis confusa.
        // Arrastre lento (< 900px/s):
        //   < 35% del slot → no snap (permite corrección fina)
        //   >= 35% → round al slot más cercano (0-49% = 0, 50-149% = 1, 150%+ = 2)
        // Flick rápido (>= 900px/s):
        //   Predicción inercial * 0.34, max 4 slots
        let flick_threshold = 900.0;
        let snap_threshold = 0.35;
        let fast_flick_threshold = 2700.0;

        let slot = if velocity.abs() > flick_threshold {
            let predicted_offset = dx + velocity * 0.34;
            let mut s = (predicted_offset / self.spacing).round() as i32;

            if s == 0 && dx.abs() > (self.spacing * 0.1) {
                s = if velocity > 0.0 { 1 } else { -1 };
            }
            let max_slots = if velocity.abs() > fast_flick_threshold { 4 } else { 2 };
            s.clamp(-max_slots, max_slots)
        } else {
            if displacement_slots.abs() > snap_threshold {
                displacement_slots.round().clamp(-2.0, 2.0) as i32
            } else {
                0
            }
        };

        log::info!(
            "Physics: snap slot={} (v={:.1}, dx={:.1}, disp_slots={:.2})",
            slot,
            velocity,
            dx,
            displacement_slots
        );
        self.snap_target = slot as f32 * self.spacing;
        self.is_moving = true;
        self.velocity = velocity;
    }
}

pub struct VerticalPhysics {
    pub offset_y: f32,
    pub velocity: f32,
    pub max_offset: f32,
    pub min_offset: f32,
    pub is_moving: bool,
    pub friction: f32,
    pub spring_k: f32,
    pub spring_c: f32,
}

impl VerticalPhysics {
    pub fn new() -> Self {
        Self {
            offset_y: 0.0,
            velocity: 0.0,
            max_offset: 0.0,
            min_offset: -1000.0, // Default, updated when list load
            is_moving: false,
            friction: 0.94,
            spring_k: 40.0,
            spring_c: 12.0,
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
        if !self.is_moving {
            return false;
        }

        let mut out_of_bounds = 0.0;
        if self.offset_y > self.max_offset {
            out_of_bounds = self.max_offset - self.offset_y;
        } else if self.offset_y < self.min_offset {
            out_of_bounds = self.min_offset - self.offset_y;
        }

        if out_of_bounds.abs() > 0.1 {
            // Spring back if out of bounds
            let accel = (self.spring_k * out_of_bounds) - (self.spring_c * self.velocity);
            self.velocity += accel * dt;
        } else {
            // Normal friction
            self.velocity *= self.friction.powf(dt * 60.0);
            if self.velocity.abs() < 5.0 {
                self.velocity = 0.0;
                self.is_moving = false;
            }
        }

        self.offset_y += self.velocity * dt;
        true
    }
}
