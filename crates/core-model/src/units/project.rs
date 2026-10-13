//! The display units a project uses for each dimension.

use super::{
    AngleFormat, AngleUnit, AreaUnit, DisplayFormat, ForceUnit, LengthUnit, MassUnit, MomentUnit,
    StressUnit, VolumeUnit,
};

/// The unit and precision a project displays each dimension in (CORE-09). The model itself
/// always stores SI values; these settings only affect text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProjectUnits {
    /// Lengths: dimensions, levels, coordinates.
    pub length: DisplayFormat<LengthUnit>,
    /// Areas: rooms, sections, reinforcement.
    pub area: DisplayFormat<AreaUnit>,
    /// Volumes: concrete, rooms.
    pub volume: DisplayFormat<VolumeUnit>,
    /// Angles: rotations, slopes.
    pub angle: AngleFormat,
    /// Forces: loads and reactions.
    pub force: DisplayFormat<ForceUnit>,
    /// Bending moments.
    pub moment: DisplayFormat<MomentUnit>,
    /// Stresses and pressures.
    pub stress: DisplayFormat<StressUnit>,
    /// Masses.
    pub mass: DisplayFormat<MassUnit>,
}

/// Common metric building practice: millimetres for lengths, square and cubic metres for
/// quantities, kilonewtons and megapascals for structure.
impl Default for ProjectUnits {
    fn default() -> Self {
        Self {
            length: DisplayFormat::new(LengthUnit::Millimetre, 0),
            area: DisplayFormat::new(AreaUnit::SquareMetre, 2),
            volume: DisplayFormat::new(VolumeUnit::CubicMetre, 3),
            angle: AngleFormat::Decimal(DisplayFormat::new(AngleUnit::Degree, 2)),
            force: DisplayFormat::new(ForceUnit::Kilonewton, 2),
            moment: DisplayFormat::new(MomentUnit::KilonewtonMetre, 2),
            stress: DisplayFormat::new(StressUnit::Megapascal, 1),
            mass: DisplayFormat::new(MassUnit::Kilogram, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Angle, Length, Stress};

    #[test]
    fn defaults_display_in_metric_building_units() {
        let units = ProjectUnits::default();
        assert_eq!(
            Length::from_si(3.2).display(units.length).to_string(),
            "3200 mm"
        );
        assert_eq!(
            Stress::from_si(2.5e7).display(units.stress).to_string(),
            "25.0 MPa"
        );
        assert_eq!(
            Angle::new(90.0, AngleUnit::Degree)
                .display_angle(units.angle)
                .to_string(),
            "90.00°"
        );
    }
}
