String formatBytes(num bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  var v = bytes.toDouble();
  var u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  if (u == 0) return '${bytes.toInt()} B';
  return '${v.toStringAsFixed(v >= 100 ? 0 : 1)} ${units[u]}';
}

String formatSpeed(num bps) => '${formatBytes(bps)}/s';

String formatEta(int remainingBytes, int bps) {
  if (bps <= 0) return '';
  final s = remainingBytes ~/ bps;
  if (s < 60) return '${s}s';
  if (s < 3600) return '${s ~/ 60}m ${s % 60}s';
  return '${s ~/ 3600}h ${(s % 3600) ~/ 60}m';
}

String formatTime(int unixSeconds) {
  final d = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  final now = DateTime.now();
  String two(int n) => n.toString().padLeft(2, '0');
  final hm = '${two(d.hour)}:${two(d.minute)}';
  if (d.year == now.year && d.month == now.month && d.day == now.day) return hm;
  return '${two(d.day)}/${two(d.month)}/${d.year} $hm';
}
