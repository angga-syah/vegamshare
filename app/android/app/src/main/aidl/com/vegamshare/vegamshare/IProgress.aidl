package com.vegamshare.vegamshare;

oneway interface IProgress {
    void onProgress(long doneBytes, long totalBytes, String current);
}
