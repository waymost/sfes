// @source: adr-0001
// @source: adr-0020
// @source: adr-0021
//! The deadline table: the next time each scheduled event is due, and the earliest of them
//! (technical design section 4; ADR 0001, point 3; ADR 0020, point 9; ADR 0021, point 9).

use sfes_api::Cycles;

/// The due time of an event that is not scheduled.
pub(crate) const NEVER: Cycles = Cycles(u64::MAX);

/// The scheduled events, in the order events due at the same time are serviced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Deadline {
    /// The DRAM refresh pause (ADR 0019, point 6).
    Refresh,
    /// The start of the next line.
    Line,
    /// The NMI flag, at H=0.5 of the line that starts vertical blank (ADR 0021, point 4).
    Nmi,
    /// The next H/V timer trigger point (ADR 0021, point 5).
    Irq,
    /// The counter latch of a `$4201` write (ADR 0021, point 7).
    Latch,
}

impl Deadline {
    /// Every event, in service order.
    const ALL: [Self; 5] = [Self::Refresh, Self::Line, Self::Nmi, Self::Irq, Self::Latch];

    /// The event's slot in the table.
    // @source: adr-0020
    // @source: adr-0021
    const fn index(self) -> usize {
        match self {
            Self::Refresh => 0,
            Self::Line => 1,
            Self::Nmi => 2,
            Self::Irq => 3,
            Self::Latch => 4,
        }
    }
}

/// The time each event is next due, and the earliest of those times.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Deadlines {
    due: [Cycles; Deadline::ALL.len()],
    next: Cycles,
}

impl Deadlines {
    /// A table with no event due.
    // @source: adr-0020
    // @source: adr-0021
    pub(crate) const fn new() -> Self {
        Self {
            due: [NEVER; Deadline::ALL.len()],
            next: NEVER,
        }
    }

    /// The earliest time any event is due: `next_deadline` in the technical design.
    // @source: adr-0001
    // @source: adr-0020
    pub(crate) const fn next(&self) -> Cycles {
        self.next
    }

    /// When `deadline` is next due.
    // @source: adr-0020
    pub(crate) const fn due(&self, deadline: Deadline) -> Cycles {
        self.due[deadline.index()]
    }

    /// Schedules `deadline` at `at`, or never at [`NEVER`].
    // @source: adr-0020
    pub(crate) const fn set(&mut self, deadline: Deadline, at: Cycles) {
        self.due[deadline.index()] = at;
        let mut next = self.due[0];
        let mut index = 1;
        while index < self.due.len() {
            if self.due[index].0 < next.0 {
                next = self.due[index];
            }
            index += 1;
        }
        self.next = next;
    }

    /// The event to service first at `now`: the earliest due at or before it, and of those
    /// due at the same time the first in [`Deadline::ALL`] (ADR 0020, point 9).
    // @source: adr-0020
    pub(crate) fn first_due(&self, now: Cycles) -> Option<Deadline> {
        if now < self.next {
            return None;
        }
        Deadline::ALL
            .into_iter()
            .filter(|deadline| self.due(*deadline) <= now)
            .min_by_key(|deadline| self.due(*deadline))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    // @source: adr-0001
    // @source: adr-0020
    #[test]
    fn the_earliest_due_event_is_serviced_first() {
        let mut deadlines = Deadlines::new();
        assert_eq!(deadlines.first_due(Cycles(u64::MAX - 1)), None);
        deadlines.set(Deadline::Refresh, Cycles(538));
        deadlines.set(Deadline::Line, Cycles(1364));
        assert_eq!(deadlines.next(), Cycles(538));
        assert_eq!(deadlines.first_due(Cycles(537)), None);
        assert_eq!(deadlines.first_due(Cycles(2000)), Some(Deadline::Refresh));
        deadlines.set(Deadline::Refresh, Cycles(1500));
        assert_eq!(deadlines.next(), Cycles(1364));
        assert_eq!(deadlines.first_due(Cycles(2000)), Some(Deadline::Line));
    }

    // @source: adr-0020
    // @source: adr-0021
    #[test]
    fn events_due_together_are_serviced_in_table_order() {
        let mut deadlines = Deadlines::new();
        for deadline in [
            Deadline::Latch,
            Deadline::Irq,
            Deadline::Nmi,
            Deadline::Line,
            Deadline::Refresh,
        ] {
            deadlines.set(deadline, Cycles(100));
        }
        let mut order = Vec::new();
        while let Some(deadline) = deadlines.first_due(Cycles(100)) {
            order.push(deadline);
            deadlines.set(deadline, NEVER);
        }
        assert_eq!(order, Deadline::ALL);
    }
}
