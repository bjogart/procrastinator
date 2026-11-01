# PostToolUse hook for Write|Edit: rustfmt edited .rs files and flag characters
# that `cargo xtask test` rejects (non-letters outside Latin-1).
$ErrorActionPreference = 'Stop'
$payload = [Console]::In.ReadToEnd() | ConvertFrom-Json
$file = $payload.tool_input.file_path
if (-not $file -or -not (Test-Path -LiteralPath $file)) { exit 0 }

$ext = [IO.Path]::GetExtension($file).ToLowerInvariant()
if ($ext -eq '.rs') {
    & rustfmt --edition 2024 $file
}

if ($ext -in '.rs', '.toml', '.md', '.yml', '.yaml', '.json') {
    $bad = Select-String -LiteralPath $file -Pattern '[^\x00-\xFF\p{L}]' -Encoding utf8
    if ($bad) {
        $lines = ($bad | ForEach-Object { "  line $($_.LineNumber): $($_.Line.Trim())" }) -join "`n"
        $msg = "Non-Latin-1 non-letter characters in $file (cargo xtask test will fail). Replace them with ASCII:`n$lines"
        @{ hookSpecificOutput = @{ hookEventName = 'PostToolUse'; additionalContext = $msg } } |
            ConvertTo-Json -Compress -Depth 4
    }
}
exit 0
