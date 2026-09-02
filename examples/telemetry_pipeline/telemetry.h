#ifndef ALTC_EXAMPLE_TELEMETRY_H
#define ALTC_EXAMPLE_TELEMETRY_H

typedef struct telemetry_record_t {
  int value;
  int weight;
} telemetry_record_t;

enum telemetry_state {
  TELEMETRY_READY,
  TELEMETRY_STOPPED,
};

extern int telemetry_batches;
extern const int telemetry_limit;

int telemetry_apply(int (*callback)(int), int value);

#endif
