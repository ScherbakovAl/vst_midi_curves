/// Структура для представления контрольной точки кривой Безье
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlPoint {
    /// Позиция точки (x, y)
    pub position: (f32, f32),
    /// Входящая касательная
    pub handle_in: (f32, f32),
    /// Исходящая касательная
    pub handle_out: (f32, f32),
}

impl ControlPoint {
    /// Создает новую контрольную точку в позиции position
    pub fn new(position: (f32, f32)) -> Self {
        Self {
            position,
            handle_in: (0.0, 0.0),
            handle_out: (0.0, 0.0),
        }
    }

    /// Ограничивает координаты точки диапазоном MIDI velocity
    pub fn clamp_to_midi_range(&mut self) {
        self.position.0 = self.position.0.clamp(0.0, 127.0);
        self.position.1 = self.position.1.clamp(0.0, 127.0);
    }

    /// Обновляет позицию точки и автоматически обновляет касательные
    pub fn set_position(&mut self, new_position: (f32, f32)) {
        let delta_x = new_position.0 - self.position.0;
        let delta_y = new_position.1 - self.position.1;
        
        self.position = new_position;
        
        // Автоматически обновляем касательные относительно новой позиции
        self.handle_in = (self.handle_in.0 + delta_x, self.handle_in.1 + delta_y);
        self.handle_out = (self.handle_out.0 + delta_x, self.handle_out.1 + delta_y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_point_creation() {
        let point = ControlPoint::new((64.0, 64.0));
        assert_eq!(point.position, (64.0, 64.0));
    }
}