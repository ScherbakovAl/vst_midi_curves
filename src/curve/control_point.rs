//! Структура для представления контрольной точки кривой Безье

use kurbo::Point;

#[derive(Debug, Clone)]
pub struct ControlPoint {
    pub position: Point,
    pub handle_in: Point,
    pub handle_out: Point,
    pub symmetric_handles: bool,
}

impl ControlPoint {
    pub fn new(position: Point) -> Self {
        Self {
            position,
            handle_in: Point::new(position.x - 20.0, position.y),
            handle_out: Point::new(position.x + 20.0, position.y),
            symmetric_handles: true,
        }
    }

    pub fn with_handles(mut self, handle_in: Point, handle_out: Point) -> Self {
        self.handle_in = handle_in;
        self.handle_out = handle_out;
        self
    }

    pub fn set_symmetric_handles(&mut self, symmetric: bool) {
        self.symmetric_handles = symmetric;
        if symmetric {
            // При включении симметрии выравниваем касательные
            let dx_in = self.handle_in.x - self.position.x;
            let dy_in = self.handle_in.y - self.position.y;
            self.handle_out = Point::new(
                self.position.x - dx_in,
                self.position.y - dy_in,
            );
        }
    }

    pub fn update_position(&mut self, new_position: Point) {
        let delta_x = new_position.x - self.position.x;
        let delta_y = new_position.y - self.position.y;

        self.position = new_position;
        self.handle_in = Point::new(
            self.handle_in.x + delta_x,
            self.handle_in.y + delta_y,
        );
        self.handle_out = Point::new(
            self.handle_out.x + delta_x,
            self.handle_out.y + delta_y,
        );
    }
}