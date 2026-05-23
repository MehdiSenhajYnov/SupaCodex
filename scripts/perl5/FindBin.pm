package FindBin;

use strict;
use warnings;

use Cwd qw(abs_path getcwd);
use File::Basename qw(basename dirname);

our ($Bin, $Dir, $RealBin, $RealDir, $Script, $RealScript);

sub _resolve_script_path {
    my $script = $0;

    return abs_path($script) if defined $script && $script =~ m{/};
    return abs_path(getcwd() . "/" . ($script // q{}));
}

sub import {
    my $resolved = _resolve_script_path();
    my $original = $0 // q{};

    $Script = basename($original);
    $RealScript = basename($resolved // $original);
    $Bin = dirname($resolved // $original);
    $Dir = $Bin;
    $RealBin = $Bin;
    $RealDir = $RealBin;
}

import();

1;
