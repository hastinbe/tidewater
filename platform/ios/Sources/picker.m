// The document picker for the first-run import screen (`docs/design/ios.md`). SDL has no file dialogs on iOS, so the
// Rust shell (`tidewater_ios_pick` in crates/client/src/shell/sdl.rs) calls this: it shows a
// UIDocumentPickerViewController for a folder or any file, opens the chosen item in place (no copy) and reports its
// path. The import reads from that path over several seconds, so the security-scoped access is kept until the app
// ends.

#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

typedef void (*TWPickDone)(const char *path);

@interface TWPicker : NSObject <UIDocumentPickerDelegate>
@property(nonatomic, assign) TWPickDone done;
@end

// The delegate is weakly held by the picker, so it lives here until it has answered.
static TWPicker *gPicker;

@implementation TWPicker

- (void)finish:(const char *)path {
    TWPickDone done = self.done;
    gPicker = nil;
    if (done) {
        done(path);
    }
}

- (void)documentPicker:(UIDocumentPickerViewController *)controller didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
    NSURL *url = urls.firstObject;
    if (url == nil) {
        [self finish:NULL];
        return;
    }
    // Never balanced with stopAccessing...: the import runs after this returns and the item is only read.
    [url startAccessingSecurityScopedResource];
    [self finish:url.fileSystemRepresentation];
}

- (void)documentPickerWasCancelled:(UIDocumentPickerViewController *)controller {
    [self finish:NULL];
}

@end

static UIViewController *TopViewController(void) {
    UIWindow *window = nil;
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:UIWindowScene.class]) {
            continue;
        }
        for (UIWindow *candidate in ((UIWindowScene *)scene).windows) {
            if (candidate.isKeyWindow) {
                window = candidate;
            }
        }
    }
    if (window == nil) {
        window = UIApplication.sharedApplication.windows.firstObject;
    }
    UIViewController *top = window.rootViewController;
    while (top.presentedViewController != nil) {
        top = top.presentedViewController;
    }
    return top;
}

// Shows the picker for a folder (`folder` non-zero) or any file. Calls `done` once, on the main thread, with the chosen
// item's path (valid for the call), or NULL if the user cancelled or no picker could be shown.
void tidewater_ios_pick(int folder, TWPickDone done) {
    UIViewController *top = TopViewController();
    if (top == nil || gPicker != nil) {
        if (done) {
            done(NULL);
        }
        return;
    }
    UTType *type = folder ? UTTypeFolder : UTTypeData;
    UIDocumentPickerViewController *picker =
        [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:@[ type ] asCopy:NO];
    TWPicker *delegate = [[TWPicker alloc] init];
    delegate.done = done;
    gPicker = delegate;
    picker.delegate = delegate;
    picker.allowsMultipleSelection = NO;
    [top presentViewController:picker animated:YES completion:nil];
}
