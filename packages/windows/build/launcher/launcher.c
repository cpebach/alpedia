/*
 * Alpedia ランチャー（Windows）
 *
 * 同じフォルダの runtime\python312.dll を読み込み、boot.py を実行するだけの小さな exe。
 * Python のインストールは不要。runtime\python312._pth が sys.path を決める（同梱のものだけを使う）。
 *
 *   Alpedia.exe          … ウィンドウ版（コンソールを出さない）  -DALPEDIA_GUI
 *   Alpedia-console.exe  … コンソール版（ログを見たい時・不具合の調査用）
 *
 * ビルド（どの OS でも可）： python -m ziglang cc -target x86_64-windows-gnu ...（build_launcher.py を参照）
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <stdlib.h>
#include <wchar.h>

typedef int (*py_main_fn)(int, wchar_t **);

static void fail(const wchar_t *msg, const wchar_t *detail) {
    wchar_t buf[2048];
    _snwprintf(buf, 2047, L"%ls\n\n%ls", msg, detail ? detail : L"");
    buf[2047] = 0;
    MessageBoxW(NULL, buf, L"Alpedia", MB_OK | MB_ICONERROR);
}

static int run(void) {
    wchar_t exe[MAX_PATH * 4], base[MAX_PATH * 4], runtime[MAX_PATH * 4], dll[MAX_PATH * 4], boot[MAX_PATH * 4];
    DWORD n = GetModuleFileNameW(NULL, exe, MAX_PATH * 4);
    if (n == 0 || n >= MAX_PATH * 4) {
        fail(L"実行ファイルの場所が分かりませんでした。", NULL);
        return 1;
    }
    wcscpy(base, exe);
    wchar_t *slash = wcsrchr(base, L'\\');
    if (slash) *slash = 0;
    _snwprintf(runtime, MAX_PATH * 4, L"%ls\\runtime", base);
    _snwprintf(dll, MAX_PATH * 4, L"%ls\\python312.dll", runtime);
    _snwprintf(boot, MAX_PATH * 4, L"%ls\\boot.py", base);

    /* python312.dll と同じフォルダの DLL（VCRUNTIME など）を先に探させる */
    SetDllDirectoryW(runtime);
    HMODULE py = LoadLibraryExW(dll, NULL, LOAD_WITH_ALTERED_SEARCH_PATH);
    if (!py) {
        wchar_t err[64];
        _snwprintf(err, 63, L"エラーコード %lu", GetLastError());
        fail(L"runtime\\python312.dll を読み込めませんでした。\nフォルダごと展開してから起動してください。", err);
        return 1;
    }
    py_main_fn py_main = (py_main_fn)GetProcAddress(py, "Py_Main");
    if (!py_main) {
        fail(L"python312.dll に Py_Main がありません。", NULL);
        return 1;
    }
    if (GetFileAttributesW(boot) == INVALID_FILE_ATTRIBUTES) {
        fail(L"boot.py が見つかりません。フォルダの中身がそろっているか確認してください。", boot);
        return 1;
    }

    int argc = 0;
    wchar_t **argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    int extra = argc > 1 ? argc - 1 : 0;
    wchar_t **args = (wchar_t **)calloc((size_t)(extra + 6), sizeof(wchar_t *));
    int k = 0;
    args[k++] = exe;
    args[k++] = L"-X";
    args[k++] = L"utf8";
    args[k++] = L"-u";
    args[k++] = boot;
    for (int i = 1; i < argc; ++i) args[k++] = argv[i];
    args[k] = NULL;
    return py_main(k, args);
}

#ifdef ALPEDIA_GUI
int WINAPI wWinMain(HINSTANCE a, HINSTANCE b, PWSTR c, int d) {
    (void)a; (void)b; (void)c; (void)d;
    return run();
}
#else
int wmain(void) {
    SetConsoleOutputCP(CP_UTF8);
    return run();
}
#endif
