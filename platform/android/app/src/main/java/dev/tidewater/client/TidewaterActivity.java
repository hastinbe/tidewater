package dev.tidewater.client;

import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.util.Log;
import android.widget.Toast;

import org.libsdl.app.SDLActivity;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Tidewater's activity (docs/design/android.md): SDL's activity with three changes.
 *
 * <ul>
 * <li>It loads only libmain.so, which has SDL linked in.
 * <li>It passes the intent's "args" string array to the program as its command line
 *     ({@code adb shell am start -n dev.tidewater.client/.TidewaterActivity --esa args "--goto,menu"}).
 * <li>It is the first-run screen's file picker. SDL 3's own file dialog on Android has no folder choice and returns
 *     {@code content:} addresses the program cannot read, so the program asks for this one (a message from SDL's
 *     {@code SDL_SendAndroidMessage}), the Storage Access Framework picks, and this class copies what was chosen into
 *     the app's cache. It writes the copy's path to {@code pick-result.txt} in the app's files folder (empty when
 *     nothing was chosen) and calls {@link #nativePicked()}, which is in libmain.so.
 * </ul>
 */
public class TidewaterActivity extends SDLActivity {
    private static final String TAG = "Tidewater";

    // The program's messages (crates/client/src/shell/sdl.rs has the same two numbers). SDL delivers numbers from
    // 0x8000 up to onUnhandledMessage.
    private static final int MSG_PICK_FOLDER = 0x8001;
    private static final int MSG_PICK_FILE = 0x8002;

    private static final int REQUEST_FOLDER = 0x7101;
    private static final int REQUEST_FILE = 0x7102;

    /** Tells the program the picker has finished (its answer is in pick-result.txt). Found in libmain.so by name. */
    private static native void nativePicked();

    @Override
    protected String[] getLibraries() {
        return new String[] {"main"};
    }

    @Override
    protected String[] getArguments() {
        String[] args = getIntent() == null ? null : getIntent().getStringArrayExtra("args");
        return args == null ? new String[0] : args;
    }

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // A copy left by an earlier run is not needed any more: the import made its own.
        deleteTree(pickedDir());
    }

    @Override
    protected boolean onUnhandledMessage(int command, Object param) {
        switch (command) {
            case MSG_PICK_FOLDER:
                startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), REQUEST_FOLDER);
                return true;
            case MSG_PICK_FILE: {
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType("*/*");
                startActivityForResult(intent, REQUEST_FILE);
                return true;
            }
            default:
                return super.onUnhandledMessage(command, param);
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        if (requestCode != REQUEST_FOLDER && requestCode != REQUEST_FILE) {
            super.onActivityResult(requestCode, resultCode, data);
            return;
        }
        final Uri uri = (resultCode == RESULT_OK && data != null) ? data.getData() : null;
        if (uri == null) {
            finishPick(null);
            return;
        }
        final boolean folder = requestCode == REQUEST_FOLDER;
        Toast.makeText(this, R.string.copying, Toast.LENGTH_LONG).show();
        new Thread(
                        () -> {
                            File result = null;
                            try {
                                File dir = pickedDir();
                                deleteTree(dir);
                                result = folder ? copyTree(uri, dir) : copyFile(uri, dir);
                            } catch (IOException | RuntimeException e) {
                                Log.e(TAG, "copying the picked item failed", e);
                                runOnUiThread(
                                        () -> Toast.makeText(this, R.string.copy_failed, Toast.LENGTH_LONG).show());
                            }
                            finishPick(result);
                        },
                        "tidewater-pick-copy")
                .start();
    }

    /** Where picked items are copied: the app's cache, which the system may clear when space runs short. */
    private File pickedDir() {
        return new File(getCacheDir(), "picked");
    }

    /** Writes the answer for the program (the path, or nothing) and tells it. */
    private void finishPick(File result) {
        try (OutputStream out = new FileOutputStream(new File(getFilesDir(), "pick-result.txt"))) {
            if (result != null) {
                out.write(result.getAbsolutePath().getBytes("UTF-8"));
            }
        } catch (IOException e) {
            Log.e(TAG, "could not write the pick result", e);
        }
        nativePicked();
    }

    /** Copies the chosen file into {@code dir}, keeping its name. */
    private File copyFile(Uri uri, File dir) throws IOException {
        String name = null;
        try (Cursor c = getContentResolver().query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                name = c.getString(0);
            }
        }
        File out = new File(dir, safeName(name, "picked-file"));
        copyStream(getContentResolver(), uri, out);
        return out;
    }

    /** Copies the chosen folder and everything in it into a folder of the same name under {@code dir}. */
    private File copyTree(Uri tree, File dir) throws IOException {
        String rootId = DocumentsContract.getTreeDocumentId(tree);
        String name = null;
        Uri root = DocumentsContract.buildDocumentUriUsingTree(tree, rootId);
        try (Cursor c = getContentResolver()
                .query(root, new String[] {DocumentsContract.Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                name = c.getString(0);
            }
        }
        File out = new File(dir, safeName(name, "picked-folder"));
        copyChildren(tree, rootId, out);
        return out;
    }

    private void copyChildren(Uri tree, String parentId, File into) throws IOException {
        if (!into.isDirectory() && !into.mkdirs()) {
            throw new IOException("cannot create " + into);
        }
        ContentResolver resolver = getContentResolver();
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parentId);
        String[] columns = {
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        };
        try (Cursor c = resolver.query(children, columns, null, null, null)) {
            if (c == null) {
                return;
            }
            while (c.moveToNext()) {
                String id = c.getString(0);
                File target = new File(into, safeName(c.getString(1), "file"));
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(c.getString(2))) {
                    copyChildren(tree, id, target);
                } else {
                    copyStream(resolver, DocumentsContract.buildDocumentUriUsingTree(tree, id), target);
                }
            }
        }
    }

    private static void copyStream(ContentResolver resolver, Uri from, File to) throws IOException {
        File parent = to.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
            throw new IOException("cannot create " + parent);
        }
        try (InputStream in = resolver.openInputStream(from);
                OutputStream out = new FileOutputStream(to)) {
            if (in == null) {
                throw new IOException("cannot open " + from);
            }
            byte[] buffer = new byte[1 << 16];
            int n;
            while ((n = in.read(buffer)) > 0) {
                out.write(buffer, 0, n);
            }
        }
    }

    /** A file name that stays inside its folder. */
    private static String safeName(String name, String fallback) {
        if (name == null) {
            return fallback;
        }
        String clean = name.replace('/', '_').replace('\0', '_').trim();
        return (clean.isEmpty() || clean.equals(".") || clean.equals("..")) ? fallback : clean;
    }

    private static void deleteTree(File f) {
        File[] kids = f.listFiles();
        if (kids != null) {
            for (File k : kids) {
                deleteTree(k);
            }
        }
        f.delete();
    }
}
