import 'dart:io';

import 'package:onde_cli/onde_cli.dart';

Future<void> main(List<String> arguments) async {
  try {
    final exitCode = await runOndeCli(arguments);
    exit(exitCode);
  } on OndeCliException catch (error) {
    stderr.writeln('onde: ${error.message}');
    exit(1);
  }
}
