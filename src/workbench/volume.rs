pub const STEP_PERCENT: u16 = 5;
pub const MAX_PERCENT: u16 = 100;

pub fn percent(volume: u16) -> u16 {
    ((volume as u32 * MAX_PERCENT as u32 + u16::MAX as u32 / 2) / u16::MAX as u32) as u16
}

pub fn from_percent(percent: u16) -> u16 {
    let percent = percent.min(MAX_PERCENT);
    ((percent as u32 * u16::MAX as u32 + MAX_PERCENT as u32 / 2) / MAX_PERCENT as u32) as u16
}

pub fn quantize_percent(percent: u16) -> u16 {
    let percent = percent.min(MAX_PERCENT);
    (((percent + STEP_PERCENT / 2) / STEP_PERCENT) * STEP_PERCENT).min(MAX_PERCENT)
}

pub fn stepped(volume: u16, delta_percent: i16) -> u16 {
    let current = quantize_percent(percent(volume)) as i16;
    let target = (current + delta_percent).clamp(0, MAX_PERCENT as i16) as u16;
    from_percent(target)
}

pub fn from_ratio(ratio: f64) -> u16 {
    let percent = (ratio.clamp(0.0, 1.0) * MAX_PERCENT as f64).round() as u16;
    from_percent(quantize_percent(percent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_exact_five_percent_points() {
        assert_eq!(percent(stepped(from_percent(0), 5)), 5);
        assert_eq!(percent(stepped(from_percent(5), 5)), 10);
        assert_eq!(percent(stepped(from_percent(95), 5)), 100);
        assert_eq!(percent(stepped(from_percent(100), 5)), 100);
        assert_eq!(percent(stepped(from_percent(5), -5)), 0);
        assert_eq!(percent(stepped(from_percent(0), -5)), 0);
    }

    #[test]
    fn ratios_snap_to_five_percent_grid() {
        assert_eq!(percent(from_ratio(0.53)), 55);
        assert_eq!(percent(from_ratio(0.02)), 0);
        assert_eq!(percent(from_ratio(0.98)), 100);
    }
}
