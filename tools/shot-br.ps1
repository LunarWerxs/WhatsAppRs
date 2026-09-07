param([Parameter(Mandatory)][string]$Out, [double]$WFrac = 0.35, [double]$HFrac = 0.45)
# Screenshot the bottom-right corner of the primary screen, where Windows draws toasts.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
$g.Dispose()
$cw = [int]($b.Width * $WFrac)
$ch = [int]($b.Height * $HFrac)
$x = $b.Width - $cw
$y = $b.Height - $ch
$rect = New-Object System.Drawing.Rectangle($x, $y, $cw, $ch)
$crop = $bmp.Clone($rect, $bmp.PixelFormat)
$crop.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$crop.Dispose(); $bmp.Dispose()
"saved $Out ($cw x $ch at $x,$y of $($b.Width)x$($b.Height))"
