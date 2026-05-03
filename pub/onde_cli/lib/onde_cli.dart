import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

class OndeCliException implements Exception {
  OndeCliException(this.message);

  final String message;

  @override
  String toString() => message;
}

Future<int> runOndeCli(List<String> arguments) async {
  final executablePath = await resolveExecutablePath();
  await ensureExecutablePermissions(executablePath);

  final process = await Process.start(
    executablePath,
    arguments,
    mode: ProcessStartMode.inheritStdio,
    workingDirectory: Directory.current.path,
  );

  return process.exitCode;
}

Future<String> resolveExecutablePath() async {
  final packageUri = await Isolate.resolvePackageUri(
    Uri.parse('package:onde_cli/onde_cli.dart'),
  );

  if (packageUri == null) {
    throw OndeCliException(
      'Failed to resolve the installed Dart package path.',
    );
  }

  final packageRootUri = packageUri.resolve('../');
  final executableUri = packageRootUri.resolve(
    'native/${runtimeIdentifier()}/${executableName()}',
  );
  final executablePath = executableUri.toFilePath();

  if (!await File(executablePath).exists()) {
    throw OndeCliException(
      "The native onde executable for '${runtimeIdentifier()}' is not bundled in this package.",
    );
  }

  return executablePath;
}

String runtimeIdentifier() {
  final architecture = architectureName();

  if (Platform.isMacOS) {
    return 'darwin-$architecture';
  }
  if (Platform.isWindows) {
    return 'windows-$architecture';
  }
  if (Platform.isLinux) {
    return 'linux-$architecture';
  }

  throw OndeCliException(
    'onde does not support this operating system through the Dart package.',
  );
}

String architectureName() {
  final abi = Abi.current().toString().toLowerCase();
  if (abi.contains('arm64')) {
    return 'arm64';
  }
  if (abi.contains('x64')) {
    return 'x64';
  }

  throw OndeCliException(
    'onde does not support the current architecture through the Dart package.',
  );
}

String executableName() => Platform.isWindows ? 'onde.exe' : 'onde';

Future<void> ensureExecutablePermissions(String executablePath) async {
  if (Platform.isWindows) {
    return;
  }

  final result = await Process.run('chmod', ['755', executablePath]);
  if (result.exitCode != 0) {
    throw OndeCliException(
      'Failed to mark the native onde binary as executable.',
    );
  }
}
