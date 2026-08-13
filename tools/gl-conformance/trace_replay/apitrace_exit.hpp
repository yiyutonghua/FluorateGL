#pragma once

#ifdef exit
#pragma push_macro("exit")
#undef exit
#define FLUORATEGL_TRACE_RESTORE_EXIT_MACRO
#endif

#include <cstdlib>
#include <stdlib.h>

#ifdef FLUORATEGL_TRACE_RESTORE_EXIT_MACRO
#pragma pop_macro("exit")
#undef FLUORATEGL_TRACE_RESTORE_EXIT_MACRO
#endif

struct FluorateGLRetraceExit {
    int status;
};

extern "C" [[noreturn]] void fluorategl_apitrace_exit(int status);
