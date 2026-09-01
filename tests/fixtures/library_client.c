#include "library.h"

#include <stdint.h>

int main(void) {
  int32_t value = 0;
  altc_ApiError error = altc_ApiError_Negative;

  if (checked_double(INT32_C(7), &value, &error) != INT32_C(0)) {
    return 1;
  }
  if (value != INT32_C(14)) {
    return 2;
  }
  if (checked_double(-INT32_C(1), &value, &error) != INT32_C(1)) {
    return 3;
  }
  return error == altc_ApiError_Negative ? 0 : 4;
}
