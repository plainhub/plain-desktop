#include "onnxruntime_c_api.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include <windows.h>
#else
#include <dlfcn.h>
#endif

typedef struct {
    const char *name;
    void *data;
    size_t bytes;
    const int64_t *shape;
    size_t rank;
    int type;
} PlainOnnxInput;
typedef struct {
    const OrtApi *api;
    OrtEnv *env;
    OrtSession *session;
    OrtMemoryInfo *memory;
    void *library;
} PlainOnnxSession;

static int check(PlainOnnxSession *s, OrtStatus *status, char *error, size_t capacity) {
    if (!status) return 0;
    snprintf(error, capacity, "%s", s->api->GetErrorMessage(status));
    s->api->ReleaseStatus(status);
    return -1;
}
void plain_onnx_close(PlainOnnxSession *s) {
    if (!s) return;
    if (s->session) s->api->ReleaseSession(s->session);
    if (s->memory) s->api->ReleaseMemoryInfo(s->memory);

    free(s);
}
PlainOnnxSession *plain_onnx_context(const char *library, char *error, size_t capacity) {
    PlainOnnxSession *s = calloc(1, sizeof(*s));
    if (!s) { snprintf(error, capacity, "ONNX allocation failed"); return NULL; }
    const OrtApiBase *base = NULL;
#ifdef PLAIN_ONNX_STATIC
    base = OrtGetApiBase();
#else
#ifdef _WIN32
    int count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, library, -1, NULL, 0);
    wchar_t *wide_library = count > 0 ? calloc(count, sizeof(wchar_t)) : NULL;
    if (wide_library) {
        MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, library, -1, wide_library, count);
        s->library = (void *)LoadLibraryW(wide_library);
        free(wide_library);
    }
    const OrtApiBase *(ORT_API_CALL *get_api)(void) = s->library ? (void *)GetProcAddress((HMODULE)s->library, "OrtGetApiBase") : NULL;
#else
    s->library = dlopen(library, RTLD_NOW | RTLD_LOCAL);
    const OrtApiBase *(*get_api)(void) = s->library ? (void *)dlsym(s->library, "OrtGetApiBase") : NULL;
#endif
    if (get_api) base = get_api();
#endif
    if (!base || !(s->api = base->GetApi(ORT_API_VERSION))) {
        snprintf(error, capacity, "ONNX Runtime %d library unavailable: %s", ORT_API_VERSION, library);
        plain_onnx_close(s); return NULL;
    }
    OrtStatus *status = s->api->CreateEnv(ORT_LOGGING_LEVEL_WARNING, "plain-image-search", &s->env);
    if (check(s, status, error, capacity)) { plain_onnx_close(s); return NULL; }
    return s;
}
PlainOnnxSession *plain_onnx_create(PlainOnnxSession *context, const char *model,
    int threads, const char *provider, char *error, size_t capacity) {
    PlainOnnxSession *s = calloc(1, sizeof(*s));
    OrtSessionOptions *options = NULL;
    if (!s) { snprintf(error, capacity, "ONNX allocation failed"); return NULL; }
    s->api = context->api;
    s->env = context->env;
#define TRY(expr) if (check(s, (expr), error, capacity)) goto failed
    TRY(s->api->CreateSessionOptions(&options));
    TRY(s->api->SetIntraOpNumThreads(options, threads));
    TRY(s->api->SetInterOpNumThreads(options, 1));
    TRY(s->api->SetSessionGraphOptimizationLevel(options, ORT_ENABLE_ALL));
    TRY(s->api->DisableCpuMemArena(options));
    if (strcmp(provider, "CPU") != 0) {
        const char *keys[2], *values[2];
        size_t count = 0;
        char thread_count[16];
        if (strcmp(provider, "CoreML") == 0) {
            keys[0] = "ModelFormat"; values[0] = "MLProgram";
            keys[1] = "MLComputeUnits"; values[1] = "ALL";
            count = 2;
        } else if (strcmp(provider, "XNNPACK") == 0) {
            snprintf(thread_count, sizeof(thread_count), "%d", threads);
            keys[0] = "intra_op_num_threads"; values[0] = thread_count;
            count = 1;
        } else {
            snprintf(error, capacity, "Unsupported ONNX execution provider"); goto failed;
        }
        TRY(s->api->SessionOptionsAppendExecutionProvider(options, provider, keys, values, count));
        if (strcmp(provider, "XNNPACK") == 0) { TRY(s->api->SetIntraOpNumThreads(options, 1)); }
    }
    TRY(s->api->CreateCpuMemoryInfo(OrtArenaAllocator, OrtMemTypeDefault, &s->memory));
#ifdef _WIN32
    int wide_count = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, model, -1, NULL, 0);
    wchar_t *wide = wide_count > 0 ? calloc(wide_count, sizeof(wchar_t)) : NULL;
    if (!wide) { snprintf(error, capacity, "Invalid ONNX model path"); goto failed; }
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, model, -1, wide, wide_count);
    OrtStatus *created = s->api->CreateSession(s->env, wide, options, &s->session);
    free(wide);
    TRY(created);
#else
    TRY(s->api->CreateSession(s->env, model, options, &s->session));
#endif
    s->api->ReleaseSessionOptions(options);
    return s;
failed:
    if (options) s->api->ReleaseSessionOptions(options);
    plain_onnx_close(s); return NULL;
#undef TRY
}
int plain_onnx_run(PlainOnnxSession *s, PlainOnnxInput *inputs, size_t count,
    const char *output_name, float *output, size_t dimensions, char *error, size_t capacity) {
    if (count == 0 || count > 4) { snprintf(error, capacity, "Invalid ONNX input count"); return -1; }
    OrtValue *values[4] = {0}, *result = NULL;
    const char *names[4] = {0};
    OrtTensorTypeAndShapeInfo *info = NULL;
    size_t input_count = 0, output_count = 0;
    int status = -1;
#define TRY(expr) if (check(s, (expr), error, capacity)) goto finished
    TRY(s->api->SessionGetInputCount(s->session, &input_count));
    TRY(s->api->SessionGetOutputCount(s->session, &output_count));
    if (input_count != count || output_count == 0) {
        snprintf(error, capacity, "ONNX encoder input/output contract mismatch"); goto finished;
    }
    for (size_t i = 0; i < count; i++) {
        names[i] = inputs[i].name;
        TRY(s->api->CreateTensorWithDataAsOrtValue(s->memory, inputs[i].data, inputs[i].bytes,
            inputs[i].shape, inputs[i].rank, (ONNXTensorElementDataType)inputs[i].type, &values[i]));
    }
    TRY(s->api->Run(s->session, NULL, names, (const OrtValue *const *)values, count,
        &output_name, 1, &result));
    TRY(s->api->GetTensorTypeAndShape(result, &info));
    ONNXTensorElementDataType type;
    size_t size = 0, rank = 0;
    TRY(s->api->GetTensorElementType(info, &type));
    TRY(s->api->GetTensorShapeElementCount(info, &size));
    TRY(s->api->GetDimensionsCount(info, &rank));
    int64_t shape[2] = {0};
    if (type != ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT || size != dimensions || rank != 2) {
        snprintf(error, capacity, "Expected ONNX float32 embedding [1,%zu]", dimensions); goto finished;
    }
    TRY(s->api->GetDimensions(info, shape, 2));
    if (shape[0] != 1 || shape[1] != (int64_t)dimensions) {
        snprintf(error, capacity, "ONNX embedding shape mismatch"); goto finished;
    }
    void *data = NULL;
    TRY(s->api->GetTensorMutableData(result, &data));
    memcpy(output, data, dimensions * sizeof(float));
    status = 0;
finished:
    if (info) s->api->ReleaseTensorTypeAndShapeInfo(info);
    if (result) s->api->ReleaseValue(result);
    for (size_t i = 0; i < count; i++) if (values[i]) s->api->ReleaseValue(values[i]);
    return status;
#undef TRY
}
