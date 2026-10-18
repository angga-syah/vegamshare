package com.vegamshare.vegamshare;

import com.vegamshare.vegamshare.IProgress;

// Runs in PrivServer, with shell (ADB) rights.
interface IPrivileged {
    // Stops the helper.
    void destroy() = 16777114;

    // Runs a shell command; returns "<exit code>\n<stdout+stderr>".
    String exec(in String[] argv) = 1;

    // Lists all files recursively under path; returns newline-separated absolute paths.
    String listFiles(String path) = 2;

    // Copies srcPath to dstPath (creates parent dirs). Returns "" on success, error message otherwise.
    String copyFile(String srcPath, String dstPath) = 3;

    // Creates a directory (and parents). Returns "" on success, error otherwise.
    String mkdirs(String path) = 4;

    // ---- Clone: paths are built here from a package name and user ids, never passed in.

    // Bytes that copyData would consider for this app in this user.
    long measure(String pkg, int user) = 5;

    // Makes sure Android/data/<pkg> exists in the target user, owned by the
    // app. 0 = already there, 1 = created, 2 = the app must be opened once.
    int prepareTarget(String pkg, int user, int appId) = 6;

    // Copies the app's game data from one user to another; files that are
    // already identical are skipped. Returns "OK|copied|bytes|same|errors|first error".
    String copyData(String pkg, int fromUser, int toUser, IProgress progress) = 7;

    void cancel() = 8;
}
