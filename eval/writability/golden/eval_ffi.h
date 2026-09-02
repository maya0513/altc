typedef struct eval_point {
  int value;
  int scale;
} eval_point_t;

enum eval_state { EVAL_READY = 1, EVAL_STOPPED = 2 };
typedef int (*eval_callback)(int);
static int eval_counter = 2;
static const int eval_limit = 5;
static inline int eval_apply(eval_callback callback, int value) {
  return callback(value);
}
