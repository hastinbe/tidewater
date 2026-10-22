// The iOS app's entry point. SDL starts UIKit (`SDL_RunApp`) and, once the app has launched, calls the Rust program
// (`tidewater_ios_main`, in crates/client/src/shell/sdl.rs): the same loop the desktop runs. The prototypes are
// written out so the project needs no SDL headers.

extern int SDL_RunApp(int argc, char *argv[], int (*main_function)(int, char *[]), void *reserved);
extern int tidewater_ios_main(int argc, char *argv[]);

int main(int argc, char *argv[]) {
    return SDL_RunApp(argc, argv, tidewater_ios_main, 0);
}
