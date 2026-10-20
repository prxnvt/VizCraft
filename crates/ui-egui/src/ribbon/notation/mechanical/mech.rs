//! Mechanical > Mechanism: Mobility (`mech.mobility`), Ratio between the two selected components
//! (`mech.ratio`), Mason's Gain (`mech.mason`) and Assign Causality (`mech.causality`). The
//! analyses fill the Analysis dock and make no undo step.

use crate::ribbon::Control::*;
use crate::ribbon::Group;

/// The Mechanism group.
pub(super) fn group() -> Group {
    Group::new(
        "Mechanism",
        vec![
            Big { id: "mech.mobility", icon: "mobility" },
            Stack(vec![
                Small { id: "mech.ratio", icon: "gearRatio" },
                Small { id: "mech.mason", icon: "mason" },
                Small { id: "mech.causality", icon: "causality" },
            ]),
        ],
    )
}
