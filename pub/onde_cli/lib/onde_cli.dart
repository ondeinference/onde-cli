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
  final version = await packageVersion();
  final cacheDirectory = await cliCacheDirectory(version);
  final executablePath = [
    cacheDirectory.path,
    runtimeIdentifier(),
    executableName(),
  ].join(Platform.pathSeparator);

  final executableFile = File(executablePath);
  if (!await executableFile.exists()) {
    await downloadExecutable(version, executableFile);
  }

  return executablePath;
}

Future<void> downloadExecutable(String version, File executableFile) async {
  final parent = executableFile.parent;
  if (!await parent.exists()) {
    await parent.create(recursive: true);
  }

  final tempFile = File('${executableFile.path}.download');
  if (await tempFile.exists()) {
    await tempFile.delete();
  }

  final client = HttpClient();
  try {
    final request = await client.getUrl(downloadUri(version));
    final response = await request.close();

    if (response.statusCode != HttpStatus.ok) {
      throw OndeCliException(
        'Failed to download onde ${version} for ${runtimeIdentifier()} '
        '(HTTP ${response.statusCode}).',
      );
    }

    final sink = tempFile.openWrite();
    await response.pipe(sink);
    await sink.close();

    if (await executableFile.exists()) {
      await executableFile.delete();
    }

    await tempFile.rename(executableFile.path);
  } on OndeCliException {
    rethrow;
  } catch (error) {
    throw OndeCliException('Failed to download the native onde binary: $error');
  } finally {
    client.close(force: true);
  }
}

Uri downloadUri(String version) {
  return Uri.parse(
    'https://github.com/ondeinference/onde-cli/releases/download/'
    'v$version/${releaseAssetName()}',
  );
}

Future<Directory> cliCacheDirectory(String version) async {
  final home = homeDirectoryPath();
  return Directory(
    [home, '.onde', 'cli', version].join(Platform.pathSeparator),
  );
}

String homeDirectoryPath() {
  final env = Platform.environment;
  final home = env['HOME'];
  if (home != null && home.isNotEmpty) {
    return home;
  }

  final userProfile = env['USERPROFILE'];
  if (userProfile != null && userProfile.isNotEmpty) {
    return userProfile;
  }

  final homeDrive = env['HOMEDRIVE'];
  final homePath = env['HOMEPATH'];
  if (homeDrive != null && homePath != null) {
    return '$homeDrive$homePath';
  }

  throw OndeCliException('Could not resolve the home directory for onde.');
}

Future<String> packageVersion() async {
  final packageRoot = await resolvePackageRoot();
  final pubspecFile = File(packageRoot.resolve('pubspec.yaml').toFilePath());

  if (!await pubspecFile.exists()) {
    throw OndeCliException('Could not read pubspec.yaml for onde_cli.');
  }

  final lines = await pubspecFile.readAsLines();
  for (final line in lines) {
    if (line.startsWith('version: ')) {
      return line.substring('version: '.length).trim().replaceAll('"', '');
    }
  }

  throw OndeCliException('Could not find a version in pubspec.yaml.');
}

Future<Uri> resolvePackageRoot() async {
  final packageUri = await Isolate.resolvePackageUri(
    Uri.parse('package:onde_cli/onde_cli.dart'),
  );

  if (packageUri == null) {
    throw OndeCliException(
      'Failed to resolve the installed Dart package path.',
    );
  }

  return packageUri.resolve('../');
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

String releaseAssetName() {
  final architecture = architectureName() == 'x64' ? 'amd64' : 'arm64';

  if (Platform.isMacOS) {
    return 'onde-macos-$architecture';
  }
  if (Platform.isLinux) {
    return 'onde-linux-$architecture';
  }
  if (Platform.isWindows) {
    return 'onde-win-$architecture.exe';
  }

  throw OndeCliException(
    'onde does not support this operating system through the Dart package.',
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
