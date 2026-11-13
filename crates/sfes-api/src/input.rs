//! Controller input types shared by every SNES input device.

bitflags::bitflags! {
    /// Standard pad buttons, in the order the pad shifts them out serially:
    /// B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R.
    #[derive(
        Clone,
        Copy,
        Debug,
        Default,
        Eq,
        Hash,
        PartialEq,
        serde::Deserialize,
        serde::Serialize,
    )]
    pub struct Buttons: u16 {
        /// B button.
        const B = 1 << 0;
        /// Y button.
        const Y = 1 << 1;
        /// Select button.
        const SELECT = 1 << 2;
        /// Start button.
        const START = 1 << 3;
        /// D-pad up.
        const UP = 1 << 4;
        /// D-pad down.
        const DOWN = 1 << 5;
        /// D-pad left.
        const LEFT = 1 << 6;
        /// D-pad right.
        const RIGHT = 1 << 7;
        /// A button.
        const A = 1 << 8;
        /// X button.
        const X = 1 << 9;
        /// L shoulder button.
        const L = 1 << 10;
        /// R shoulder button.
        const R = 1 << 11;
    }
}
