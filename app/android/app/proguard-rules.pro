# Started by name with app_process over ADB.
-keep class com.vegamshare.vegamshare.PrivServer {
    public static void main(java.lang.String[]);
}

# libadb.so binds its JNI methods to these class and method names.
-keep class moe.shizuku.manager.adb.PairingContext { *; }
-keepclasseswithmembers class moe.shizuku.manager.adb.** {
    native <methods>;
}
