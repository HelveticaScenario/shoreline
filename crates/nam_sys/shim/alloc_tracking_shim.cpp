// Test-only: counts every malloc, free and operator new/delete in the
// process while tracking is on, through NeuralAmpModelerCore's own tracker.

#include "allocation_tracking.h"

extern "C" {

void nam_alloc_tracking_begin()
{
  allocation_tracking::g_allocation_count = 0;
  allocation_tracking::g_deallocation_count = 0;
  allocation_tracking::g_tracking_enabled = true;
}

// Stops tracking and returns the allocations plus deallocations seen.
int nam_alloc_tracking_end()
{
  allocation_tracking::g_tracking_enabled = false;
  return allocation_tracking::g_allocation_count + allocation_tracking::g_deallocation_count;
}

} // extern "C"
