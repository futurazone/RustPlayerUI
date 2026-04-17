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
            lib_offset: -3,
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
        let flick_threshold = 900.0; // Sincronizado con Python (very_fast_flick)
        let commit_threshold = 0.18; // Sincronizado con commit_threshold en Python

        let slot = if velocity.abs() > flick_threshold {
            // Predicción de inercia (flick) - Usamos 0.34 como en Python
            let predicted_offset = dx + velocity * 0.34;
            let mut s = (predicted_offset / self.spacing).round() as i32;

            // Asegurar que al menos se mueva un slot en la dirección del flick si hay intención
            if s == 0 && dx.abs() > (self.spacing * 0.1) {
                s = if velocity > 0.0 { 1 } else { -1 };
            }
            s.clamp(-6, 6)
        } else {
            // Snap simple basado en desplazamiento real
            if displacement_slots > commit_threshold {
                1
            } else if displacement_slots < -commit_threshold {
                -1
            } else {
                0
            }
        };

        log::info!(
            "Physics: Snapping to slot {} (v={:.1}, dx={:.1})",
            slot,
            velocity,
            dx
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
