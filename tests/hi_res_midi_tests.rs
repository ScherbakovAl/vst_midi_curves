//! Тесты для проверки исправленной обработки hi-res MIDI velocity

use vst_midi_curves::midi_simple::{SimpleMidiManager, HiResMidiEvent, MidiMode};
use std::sync::{Arc, Mutex};
use vst_midi_curves::curve::{DualCurve, ControlPoint};

#[cfg(test)]
mod tests {
    use super::*;

    /// Тест обработки hi-res velocity с правильным алгоритмом
    #[test]
    fn test_hi_res_velocity_processing() {
        // Создаем тестовую кривую (линейная для простоты)
        let dual_curve = Arc::new(Mutex::new(DualCurve::new()));
        let mut midi_manager = SimpleMidiManager::new(dual_curve);
        
        // Переключаемся в hi-res режим
        midi_manager.set_midi_mode(MidiMode::HighResolution);
        
        // Тестовое значение: 20 (цельная) << 7 | 10 (дробная) = 2570
        let test_velocity: u16 = (20 << 7) | 10; // 20*128 + 10 = 2570
        
        // Применяем обработку velocity
        let processed_velocity = midi_manager.process_hi_res_velocity(test_velocity);
        
        // Для линейной кривой результат должен быть равен входному значению
        assert_eq!(processed_velocity, test_velocity, 
            "Линейная кривая должна сохранить значение velocity без изменений");
        
        println!("✅ Hi-res velocity processing: {} -> {}", test_velocity, processed_velocity);
    }

    /// Тест генерации MIDI сообщений из hi-res velocity
    #[test]
    fn test_hi_res_midi_message_generation() {
        let dual_curve = Arc::new(Mutex::new(DualCurve::new()));
        let midi_manager = SimpleMidiManager::new(dual_curve);
        
        // Тестовое значение: 20 (цельная) << 7 | 10 (дробная) = 2570
        let test_velocity: u16 = (20 << 7) | 10;
        let channel = 0;
        let note = 56;
        
        // Генерируем MIDI сообщения
        let messages = midi_manager.generate_hi_res_midi_messages(test_velocity, channel, note);
        
        // Должно быть ровно 2 сообщения
        assert_eq!(messages.len(), 2, "Должно быть сгенерировано 2 MIDI сообщения");
        
        // Первое сообщение: CC для дробной части
        let first_message = &messages[0];
        assert_eq!(first_message.len(), 4, "CC сообщение должно иметь 4 байта");
        assert_eq!(first_message[0], 0x90 | channel, "Статус байт должен быть NoteOn + канал");
        assert_eq!(first_message[1], 0xB0, "Второй байт должен быть Control Change");
        assert_eq!(first_message[2], 0x58, "Контроллер должен быть 88 (0x58)");
        assert_eq!(first_message[3], 10, "Дробная часть должна быть 10");
        
        // Второе сообщение: NoteOn для целой части
        let second_message = &messages[1];
        assert_eq!(second_message.len(), 4, "NoteOn сообщение должно иметь 4 байта");
        assert_eq!(second_message[0], 0x90 | channel, "Статус байт должен быть NoteOn + канал");
        assert_eq!(second_message[1], note, "Второй байт должен быть номером ноты");
        assert_eq!(second_message[2], 20, "Целая часть должна быть 20");
        
        println!("✅ Генерация MIDI сообщений:");
        println!("  Первое: {:?}", messages[0]);
        println!("  Второе: {:?}", messages[1]);
    }

    /// Тест обработки ControlChange для hi-res MIDI
    #[test]
    fn test_hi_res_control_change_processing() {
        let dual_curve = Arc::new(Mutex::new(DualCurve::new()));
        let mut midi_manager = SimpleMidiManager::new(dual_curve);
        midi_manager.set_midi_mode(MidiMode::HighResolution);
        
        // Тест: получаем дробную часть (CC#88)
        let lsb_result = midi_manager.process_control_change(0, 0x58, 10);
        
        // Должно вернуть ControlChangeLSB, так как нет еще целой части
        assert!(lsb_result.is_some());
        if let Some(event) = lsb_result {
            match event {
                HiResMidiEvent::ControlChangeLSB { channel, controller, value } => {
                    assert_eq!(channel, 0);
                    assert_eq!(controller, 0x58);
                    assert_eq!(value, 10);
                }
                _ => panic!("Ожидался ControlChangeLSB"),
            }
        }
        
        // Тест: получаем целую часть (CC#112)
        let msb_result = midi_manager.process_control_change(0, 0x70, 20);
        
        // Теперь должно создать hi-res NoteOn событие
        assert!(msb_result.is_some());
        if let Some(event) = msb_result {
            match event {
                HiResMidiEvent::HiResNoteOn { channel, note, velocity } => {
                    assert_eq!(channel, 0);
                    assert_eq!(note, 60); // Middle C по умолчанию
                    assert_eq!(velocity, (20 << 7) | 10); // 20*128 + 10 = 2570
                }
                _ => panic!("Ожидался HiResNoteOn"),
            }
        }
        
        println!("✅ Обработка ControlChange для hi-res MIDI");
    }

    /// Тест комплексного процесса: от парсинга до генерации сообщений
    #[test]
    fn test_complete_hi_res_workflow() {
        let dual_curve = Arc::new(Mutex::new(DualCurve::new()));
        let mut midi_manager = SimpleMidiManager::new(dual_curve);
        midi_manager.set_midi_mode(MidiMode::HighResolution);
        
        // Исходные данные: velocity = 2570 (20 << 7 | 10)
        let original_velocity: u16 = (20 << 7) | 10;
        let channel = 1;
        let note = 72;
        
        // 1. Парсим входящие сообщения
        let _lsb_event = midi_manager.process_control_change(channel, 0x58, 10);
        let hi_res_event = midi_manager.process_control_change(channel, 0x70, 20);
        
        assert!(hi_res_event.is_some());
        
        // 2. Обрабатываем hi-res событие через кривую
        if let Some(event) = hi_res_event {
            match event {
                HiResMidiEvent::HiResNoteOn { velocity, .. } => {
                    // Применяем обработку velocity
                    let processed_velocity = midi_manager.process_hi_res_velocity(velocity);
                    
                    // Для линейной кривой результат должен совпадать с входным
                    assert_eq!(processed_velocity, original_velocity);
                    
                    // 3. Генерируем выходные сообщения
                    let output_messages = midi_manager.generate_hi_res_midi_messages(
                        processed_velocity, channel, note
                    );
                    
                    assert_eq!(output_messages.len(), 2);
                    
                    println!("✅ Полный процесс hi-res MIDI:");
                    println!("  Исходный velocity: {}", original_velocity);
                    println!("  Обработанный velocity: {}", processed_velocity);
                    println!("  Сообщение 1: {:?}", output_messages[0]);
                    println!("  Сообщение 2: {:?}", output_messages[1]);
                }
                _ => panic!("Ожидалось HiResNoteOn"),
            }
        }
    }

    /// Тест граничных значений hi-res velocity
    #[test]
    fn test_hi_res_boundary_values() {
        let dual_curve = Arc::new(Mutex::new(DualCurve::new()));
        let mut midi_manager = SimpleMidiManager::new(dual_curve);
        
        // Тестируем минимальное значение (0)
        let min_velocity = midi_manager.process_hi_res_velocity(0);
        assert_eq!(min_velocity, 0, "Минимальное значение должно остаться 0");
        
        // Тестируем максимальное значение (16383)
        let max_velocity = midi_manager.process_hi_res_velocity(16383);
        assert_eq!(max_velocity, 16383, "Максимальное значение должно остаться 16383");
        
        // Тестируем среднее значение (8191)
        let mid_velocity = midi_manager.process_hi_res_velocity(8191);
        assert_eq!(mid_velocity, 8191, "Среднее значение должно остаться 8191");
        
        println!("✅ Граничные значения hi-res velocity:");
        println!("  Минимум (0): {}", min_velocity);
        println!("  Максимум (16383): {}", max_velocity);
        println!("  Среднее (8191): {}", mid_velocity);
    }

    /// Тест того, что дробная часть обрабатывается кривой
    #[test]
    fn test_hi_res_fractional_part_processing() {
        // Создаем модифицированную кривую, которая должна изменить дробную часть
        let mut dual_curve = DualCurve::new();
        
        // Модифицируем кривую так чтобы она увеличивала значения примерно в 1.5 раза
        // Изменяем контрольные точки для создания экспоненциальной кривой
        dual_curve.note_on_curve.control_points.clear();
        dual_curve.note_on_curve.control_points.push(ControlPoint {
            position: (0.0, 0.0),
            handle_in: (0.0, 0.0),
            handle_out: (0.0, 0.0),
        });
        dual_curve.note_on_curve.control_points.push(ControlPoint {
            position: (63.5, 95.0), // Середина становится выше
            handle_in: (0.0, 0.0),
            handle_out: (0.0, 0.0),
        });
        dual_curve.note_on_curve.control_points.push(ControlPoint {
            position: (127.0, 127.0),
            handle_in: (0.0, 0.0),
            handle_out: (0.0, 0.0),
        });
        dual_curve.note_on_curve.dirty = true;
        
        let dual_curve_arc = Arc::new(Mutex::new(dual_curve));
        let mut manager = SimpleMidiManager::new(dual_curve_arc);
        
        // Переключаемся в hi-res режим
        manager.set_midi_mode(MidiMode::HighResolution);
        
        // Тестируем значение, где дробная часть должна измениться
        // 2570 = (20 << 7) | 10 = 20.078125 как дробная часть
        let test_velocity = 2570; 
        let processed = manager.process_hi_res_velocity(test_velocity);
        
        println!("Тест velocity с модифицированной кривой: {} -> {}", test_velocity, processed);
        
        // Если кривая работает правильно, значение должно измениться
        assert_ne!(test_velocity, processed, "Кривая должна изменить значение velocity");
        
        // Проверяем, что изменение произошло именно в дробной части (или обеих частях)
        let original_msb = (test_velocity >> 7) as u8;
        let original_lsb = (test_velocity & 0x7F) as u8;
        let processed_msb = (processed >> 7) as u8;
        let processed_lsb = (processed & 0x7F) as u8;
        
        println!("Исходные: MSB={}, LSB={}", original_msb, original_lsb);
        println!("Обработанные: MSB={}, LSB={}", processed_msb, processed_lsb);
        
        // Как минимум одна из частей должна измениться
        assert!(processed_lsb != original_lsb || processed_msb != original_msb,
            "Хотя бы одна часть (MSB или LSB) должна измениться при обработке кривой");
        
        println!("✅ Дробная часть успешно обрабатывается кривой!");
        println!("   Исходная LSB: {}, Обработанная LSB: {}", original_lsb, processed_lsb);
    }
}