#ifndef LENS_LIFECYCLE_H
#define LENS_LIFECYCLE_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Start observing system lifecycle events (sleep/wake, display changes).
/// Returns true on success.
bool lens_lifecycle_start(void);

/// Stop observing.
void lens_lifecycle_stop(void);

/// Returns true if the system woke from sleep since the last poll.
/// Edge-triggered: resets after reading.
bool lens_lifecycle_poll_wake(void);

/// Returns true if the display configuration changed since the last poll.
/// Edge-triggered: resets after reading.
bool lens_lifecycle_poll_display_change(void);

#ifdef __cplusplus
}
#endif

#endif /* LENS_LIFECYCLE_H */
