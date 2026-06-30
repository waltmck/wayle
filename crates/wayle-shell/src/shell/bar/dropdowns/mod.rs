mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod calendar;
mod dashboard;
mod iwd;
mod media;
mod network;
mod notification;
mod registry;
mod weather;

pub(crate) use self::registry::{
    DropdownFactory, DropdownInstance, DropdownRegistry, dispatch_click, dispatch_click_widget,
    require_service,
};
use crate::shell::services::ShellServices;

pub(crate) fn scaled_dimension(base: f32, scale: f32) -> i32 {
    (base * scale).round() as i32
}

/// Maps a WiFi channel frequency (MHz) to its band label, shared by the
/// NetworkManager and IWD dropdowns.
pub(crate) fn frequency_to_band(freq_mhz: u32) -> Option<&'static str> {
    match freq_mhz {
        2400..=2500 => Some("2.4 GHz"),
        5000..=5900 => Some("5 GHz"),
        5901..=7125 => Some("6 GHz"),
        57000..=71000 => Some("60 GHz"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::frequency_to_band;

    #[test]
    fn frequency_2ghz_band() {
        assert_eq!(frequency_to_band(2412), Some("2.4 GHz"));
        assert_eq!(frequency_to_band(2437), Some("2.4 GHz"));
        assert_eq!(frequency_to_band(2484), Some("2.4 GHz"));
    }

    #[test]
    fn frequency_5ghz_band() {
        assert_eq!(frequency_to_band(5180), Some("5 GHz"));
        assert_eq!(frequency_to_band(5745), Some("5 GHz"));
        assert_eq!(frequency_to_band(5825), Some("5 GHz"));
    }

    #[test]
    fn frequency_6ghz_band() {
        assert_eq!(frequency_to_band(5955), Some("6 GHz"));
        assert_eq!(frequency_to_band(6115), Some("6 GHz"));
        assert_eq!(frequency_to_band(7115), Some("6 GHz"));
    }

    #[test]
    fn frequency_60ghz_band() {
        assert_eq!(frequency_to_band(60000), Some("60 GHz"));
    }

    #[test]
    fn frequency_unknown_band() {
        assert_eq!(frequency_to_band(0), None);
        assert_eq!(frequency_to_band(900), None);
    }
}

macro_rules! register_dropdowns {
    ($($name:literal => $factory:ty),+ $(,)?) => {
        pub(crate) const DROPDOWN_NAMES: &[&str] = &[$($name),+];

        pub(crate) fn create(
            name: &str,
            services: &ShellServices,
        ) -> Option<DropdownInstance> {
            match name {
                $($name => <$factory as DropdownFactory>::create(services),)+
                _ => {
                    tracing::warn!(dropdown = name, "unknown dropdown type");
                    None
                }
            }
        }
    };
}

register_dropdowns! {
    "audio" => audio::Factory,
    "battery" => battery::Factory,
    "bluetooth" => bluetooth::Factory,
    "brightness" => brightness::Factory,
    "calendar" => calendar::Factory,
    "dashboard" => dashboard::Factory,
    "iwd" => iwd::Factory,
    "media" => media::Factory,
    "network" => network::Factory,
    "notification" => notification::Factory,
    "weather" => weather::Factory,
}
