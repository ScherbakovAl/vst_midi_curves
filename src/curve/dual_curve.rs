/// Структура для управления двумя отдельными кривыми - для NoteOn и NoteOff событий
use crate::curve::{BezierCurve, ControlPoint};

/// Структура, содержащая две отдельные кривые для обработки разных типов MIDI событий
#[derive(Debug, Clone)]
pub struct DualCurve {
    /// Кривая для обработки velocity NoteOn событий
    pub note_on_curve: BezierCurve,
    /// Кривая для обработки velocity NoteOff событий
    pub note_off_curve: BezierCurve,
    /// Флаг режима MIDI высокого разрешения
    pub hi_res_enabled: bool,
}

impl DualCurve {
    /// Создает новую пару кривых с линейными кривыми по умолчанию (y = x)
    pub fn new() -> Self {
        Self {
            note_on_curve: BezierCurve::new(),
            note_off_curve: BezierCurve::new(),
            hi_res_enabled: false,
        }
    }
    
    /// Устанавливает режим MIDI высокого разрешения
    pub fn set_hi_res_enabled(&mut self, enabled: bool) {
        self.hi_res_enabled = enabled;
    }
    
    /// Возвращает состояние режима MIDI высокого разрешения
    pub fn is_hi_res_enabled(&self) -> bool {
        self.hi_res_enabled
    }

    /// Создает кривые из заданных контрольных точек
    pub fn from_points(
        note_on_points: Vec<ControlPoint>,
        note_off_points: Vec<ControlPoint>,
    ) -> Self {
        Self {
            note_on_curve: BezierCurve::from_points(note_on_points),
            note_off_curve: BezierCurve::from_points(note_off_points),
            hi_res_enabled: false,
        }
    }

    /// Обрабатывает velocity для NoteOn событий
    pub fn process_note_on_velocity(&mut self, input_velocity: u8) -> u8 {
        self.note_on_curve.evaluate(input_velocity as f32) as u8
    }

    /// Обрабатывает velocity для NoteOff событий
    pub fn process_note_off_velocity(&mut self, input_velocity: u8) -> u8 {
        self.note_off_curve.evaluate(input_velocity as f32) as u8
    }
    
    /// Обрабатывает 14-битное velocity для NoteOn событий (hi-res режим)
    /// Входное значение: 0-16383 (2^14 - 1), выходное: 0-16383
    /// ВАЖНО: Для линейной кривой сохраняет входное значение точно
    pub fn process_note_on_velocity_14bit(&mut self, input_velocity: u16) -> u16 {
        // Оптимизация: для линейной кривой возвращаем входное значение без обработки
        if self.note_on_curve.is_linear() {
            return input_velocity.min(16383);
        }
        
        // Нормализуем 14-битное значение к диапазону 0.0-1.0
        let normalized_input = input_velocity as f64 / 16383.0;
        
        // Применяем кривую в нормализованном пространстве
        let normalized_output = self.note_on_curve.evaluate_normalized(normalized_input);
        
        // Денормализуем обратно к 14-битному диапазону с округлением
        let result = (normalized_output * 16383.0).round() as u16;
        result.min(16383)
    }
    
    /// Обрабатывает 14-битное velocity для NoteOff событий (hi-res режим)
    /// Входное значение: 0-16383 (2^14 - 1), выходное: 0-16383
    /// ВАЖНО: Для линейной кривой сохраняет входное значение точно
    pub fn process_note_off_velocity_14bit(&mut self, input_velocity: u16) -> u16 {
        // Оптимизация: для линейной кривой возвращаем входное значение без обработки
        if self.note_off_curve.is_linear() {
            return input_velocity.min(16383);
        }
        
        // Нормализуем 14-битное значение к диапазону 0.0-1.0
        let normalized_input = input_velocity as f64 / 16383.0;
        
        // Применяем кривую в нормализованном пространстве
        let normalized_output = self.note_off_curve.evaluate_normalized(normalized_input);
        
        // Денормализуем обратно к 14-битному диапазону с округлением
        let result = (normalized_output * 16383.0).round() as u16;
        result.min(16383)
    }
    
    /// Объединяет 7-битные значения в 14-битное (LL - младшие, HH - старшие)
    pub fn combine_14bit(ll: u8, hh: u8) -> u16 {
        // HH содержит старшие 7 бит, LL содержит младшие 7 бит
        // Результат: HH << 7 | LL
        ((hh as u16) << 7) | (ll as u16)
    }
    
    /// Разделяет 14-битное значение на 7-битные (возвращает (LL, HH))
    pub fn split_14bit(value: u16) -> (u8, u8) {
        let ll = (value & 0x7F) as u8;  // Младшие 7 бит
        let hh = ((value >> 7) & 0x7F) as u8;  // Старшие 7 бит
        (ll, hh)
    }

    /// Сбрасывает обе кривые к линейным
    pub fn reset_to_linear(&mut self) {
        self.note_on_curve.reset_to_linear();
        self.note_off_curve.reset_to_linear();
    }

    /// Возвращает кэшированные значения для кривой NoteOn
    pub fn get_note_on_cached_values(&mut self) -> &[(f32, f32)] {
        self.note_on_curve.get_cached_values()
    }

    /// Возвращает кэшированные значения для кривой NoteOff
    pub fn get_note_off_cached_values(&mut self) -> &[(f32, f32)] {
        self.note_off_curve.get_cached_values()
    }

    /// Добавляет контрольную точку к кривой NoteOn
    pub fn add_note_on_point(&mut self, position: (f32, f32)) {
        self.note_on_curve.add_control_point(position);
    }

    /// Добавляет контрольную точку к кривой NoteOff
    pub fn add_note_off_point(&mut self, position: (f32, f32)) {
        self.note_off_curve.add_control_point(position);
    }

    /// Обновляет контрольную точку в кривой NoteOn
    pub fn update_note_on_point(&mut self, index: usize, position: (f32, f32)) -> bool {
        self.note_on_curve.update_control_point(index, position)
    }

    /// Обновляет контрольную точку в кривой NoteOff
    pub fn update_note_off_point(&mut self, index: usize, position: (f32, f32)) -> bool {
        self.note_off_curve.update_control_point(index, position)
    }

    /// Удаляет контрольную точку из кривой NoteOn
    pub fn remove_note_on_point(&mut self, index: usize) -> bool {
        self.note_on_curve.remove_control_point(index)
    }

    /// Удаляет контрольную точку из кривой NoteOff
    pub fn remove_note_off_point(&mut self, index: usize) -> bool {
        self.note_off_curve.remove_control_point(index)
    }

    /// Возвращает количество контрольных точек в кривой NoteOn
    pub fn note_on_points_count(&self) -> usize {
        self.note_on_curve.control_points.len()
    }

    /// Возвращает количество контрольных точек в кривой NoteOff
    pub fn note_off_points_count(&self) -> usize {
        self.note_off_curve.control_points.len()
    }

    /// Загружает пресет (только для кривой NoteOn в данной реализации)
    pub fn load_from_preset(&mut self, preset: &crate::presets::CurvePreset) {
        self.note_on_curve.control_points = preset.to_control_points();
        self.note_on_curve.dirty = true;
    }
}

impl Default for DualCurve {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dual_curve_creation() {
        let dual = DualCurve::new();
        assert_eq!(dual.note_on_points_count(), 2);
        assert_eq!(dual.note_off_points_count(), 2);
    }

    #[test]
    fn test_note_on_processing() {
        let mut dual = DualCurve::new();
        
        // Тест линейной кривой по умолчанию
        assert_eq!(dual.process_note_on_velocity(64), 64);
        assert_eq!(dual.process_note_on_velocity(0), 0);
        assert_eq!(dual.process_note_on_velocity(127), 127);
    }

    #[test]
    fn test_note_off_processing() {
        let mut dual = DualCurve::new();
        
        // Тест линейной кривой по умолчанию
        assert_eq!(dual.process_note_off_velocity(64), 64);
        assert_eq!(dual.process_note_off_velocity(0), 0);
        assert_eq!(dual.process_note_off_velocity(127), 127);
    }

    #[test]
    fn test_reset_to_linear() {
        let mut dual = DualCurve::new();
        
        // Добавляем точки к обеим кривым
        dual.add_note_on_point((64.0, 32.0));
        dual.add_note_off_point((32.0, 64.0));
        
        assert_eq!(dual.note_on_points_count(), 3);
        assert_eq!(dual.note_off_points_count(), 3);
        
        // Сбрасываем к линейным
        dual.reset_to_linear();
        
        assert_eq!(dual.note_on_points_count(), 2);
        assert_eq!(dual.note_off_points_count(), 2);
        
        // Проверяем что линейность восстановлена
        assert_eq!(dual.process_note_on_velocity(64), 64);
        assert_eq!(dual.process_note_off_velocity(64), 64);
    }

    #[test]
    fn test_independent_curves() {
        let mut dual = DualCurve::new();
        
        // Модифицируем только кривую NoteOn
        dual.add_note_on_point((64.0, 32.0));
        
        // NoteOn кривая должна быть нелинейной
        assert_ne!(dual.process_note_on_velocity(64), 64);
        
        // NoteOff кривая должна остаться линейной
        assert_eq!(dual.process_note_off_velocity(64), 64);
    }
}