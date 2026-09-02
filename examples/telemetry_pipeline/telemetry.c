#include "telemetry.h"

int telemetry_batches = 0;
const int telemetry_limit = 5;

int telemetry_apply(int (*callback)(int), int value) {
  return callback(value);
}
