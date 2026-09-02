#ifndef ALTC_TEST_FFI_CORE_H
#define ALTC_TEST_FFI_CORE_H

typedef struct telemetry_point {
  int value;
  int scale;
} telemetry_point_t;

enum telemetry_state {
  TELEMETRY_OK = 1,
  TELEMETRY_FAILED = 2,
};

typedef int (*telemetry_callback)(int);

static int telemetry_counter = 2;
static const int telemetry_limit = 5;

static inline int telemetry_apply(telemetry_callback callback, int value) {
  return callback(value);
}

#endif
