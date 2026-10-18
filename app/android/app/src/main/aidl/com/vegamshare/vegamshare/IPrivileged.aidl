package com.vegamshare.vegamshare;

// Runs inside Shizuku, with shell (ADB) rights.
interface IPrivileged {
    // Required by Shizuku.
    void destroy() = 16777114;

    // Runs a shell command; returns "<exit code>\n<stdout+stderr>".
    String exec(in String[] argv) = 1;

    // Lists all files recursively under path; returns newline-separated absolute paths.
    String listFiles(String path) = 2;

    // Copies srcPath to dstPath (creates parent dirs). Returns "" on success, error message otherwise.
    String copyFile(String srcPath, String dstPath) = 3;

    // Creates a directory (and parents). Returns "" on success, error otherwise.
    String mkdirs(String path) = 4;
}
