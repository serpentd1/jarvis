# Appel vocal dans l'appli WhatsApp du PC, pilotee comme le ferait une personne (recherche, Entree, bouton).
# Lance par JarvisBarre (pouvoirs.rs). La personne est donnee par la variable JARVIS_WA_CIBLE (nom ou numero).
#   -Mode ouvrir  : cherche la personne et ouvre sa discussion. Ecrit « CONTACT:<nom affiche> ».
#   -Mode appeler : verifie que la discussion ouverte est bien celle de JARVIS_WA_ATTENDU, puis clique sur
#                   « Appel vocal ». Ecrit « APPEL:<nom> ».
# Erreurs : « ERREUR:<raison> » (absent, introuvable, pas_d_appel, fenetre, recherche).
param([string]$Mode = 'ouvrir')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms, WindowsBase
Add-Type @"
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public class JarvisWa {
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(P p, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, UIntPtr e);
  public static IntPtr Trouver(string classe) {
    IntPtr r = IntPtr.Zero;
    EnumWindows((h, x) => {
      var c = new StringBuilder(256); GetClassName(h, c, 256);
      var t = new StringBuilder(256); GetWindowText(h, t, 256);
      if (c.ToString() == classe && t.ToString().Contains("WhatsApp")) { r = h; return false; }
      return true;
    }, IntPtr.Zero);
    return r;
  }
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  public static void Clic(int x, int y) { SetCursorPos(x, y); mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
  public static void Devant(IntPtr h) {
    if (IsIconic(h)) ShowWindow(h, 9);
    keybd_event(0x12, 0, 0, UIntPtr.Zero); keybd_event(0x12, 0, 2, UIntPtr.Zero);   // Alt : Windows accepte alors de changer de fenetre
    SetForegroundWindow(h);
  }
}
"@
function Fin($texte) { Write-Output $texte; exit 0 }

$cible = "$env:JARVIS_WA_CIBLE".Trim()
$attendu = "$env:JARVIS_WA_ATTENDU".Trim()
if (-not $cible) { Fin 'ERREUR:recherche' }

# 1. la fenetre de WhatsApp (on lance l'appli si besoin)
$main = [JarvisWa]::Trouver('WinUIDesktopWin32WindowClass')
if ($main -eq [IntPtr]::Zero) {
  try { Start-Process 'whatsapp:' } catch { Fin 'ERREUR:absent' }
  for ($i = 0; $i -lt 40 -and $main -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 500; $main = [JarvisWa]::Trouver('WinUIDesktopWin32WindowClass') }
  if ($main -eq [IntPtr]::Zero) { Fin 'ERREUR:absent' }
  Start-Sleep -Seconds 4
}
[JarvisWa]::Devant($main)
Start-Sleep -Milliseconds 700
$web = [IntPtr]::Zero
for ($i = 0; $i -lt 20 -and $web -eq [IntPtr]::Zero; $i++) { $web = [JarvisWa]::Trouver('Chrome_WidgetWin_1'); if ($web -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 500 } }
if ($web -eq [IntPtr]::Zero) { Fin 'ERREUR:fenetre' }
$racine = [System.Windows.Automation.AutomationElement]::FromHandle($web)

function Tous { $racine.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) }
function Genre($x) { $x.Current.ControlType.ProgrammaticName -replace 'ControlType\.', '' }
function Attendre([scriptblock]$trouver, [int]$ms) {
  $fin = (Get-Date).AddMilliseconds($ms)
  do { $x = & $trouver; if ($x) { return $x }; Start-Sleep -Milliseconds 250 } while ((Get-Date) -lt $fin)
  return $null
}
function Entete {   # « Infos du contact Maman » -> Maman (la discussion ouverte)
  $b = Tous | Where-Object { (Genre $_) -eq 'Button' -and $_.Current.Name -match '^(Infos du contact|Contact info|Infos du groupe|Group info)\s+(.+)$' } | Select-Object -First 1
  if ($b -and $b.Current.Name -match '^(Infos du contact|Contact info)\s+(.+)$') { return $Matches[2].Trim() }
  return $null
}
function BoutonAppel { Tous | Where-Object { (Genre $_) -eq 'Button' -and $_.Current.Name -match '^(Appel vocal|Voice call)$' } | Select-Object -First 1 }
function Simple($t) { ("$t".Normalize([Text.NormalizationForm]::FormD) -replace '\p{Mn}', '').ToLower() -replace '[^a-z0-9+]', '' }

function Devant-Ok { $f = [JarvisWa]::GetForegroundWindow(); return ($f -eq $main -or $f -eq $web) }

function Cliquer($x) {   # comme un clic de souris, sans jamais passer par la touche Entree
  foreach ($p in @([System.Windows.Automation.InvokePattern]::Pattern, [System.Windows.Automation.SelectionItemPattern]::Pattern)) {
    $o = $null
    if ($x.TryGetCurrentPattern($p, [ref]$o)) {
      try { if ($o -is [System.Windows.Automation.InvokePattern]) { $o.Invoke() } else { $o.Select() }; return $true } catch { }
    }
  }
  $pt = New-Object System.Windows.Point
  if ($x.TryGetClickablePoint([ref]$pt)) { [JarvisWa]::Clic([int]$pt.X, [int]$pt.Y); return $true }
  return $false
}

function Ouvrir {
  # on repart d'une recherche vide (le champ ne se montre plus quand il contient deja du texte)
  $vider = Tous | Where-Object { (Genre $_) -eq 'Button' -and $_.Current.Name -match '^(End icon button|Annuler la recherche|Cancel search)$' } | Select-Object -First 1
  if ($vider) { [void](Cliquer $vider); Start-Sleep -Milliseconds 600 }
  $champ = Attendre { Tous | Where-Object { (Genre $_) -eq 'Edit' -and $_.Current.Name -match 'Rechercher|Search' } | Select-Object -First 1 } 6000
  if (-not $champ) { Fin 'ERREUR:recherche' }
  $champ.SetFocus(); Start-Sleep -Milliseconds 300
  if (-not (Devant-Ok)) { [JarvisWa]::Devant($main); Start-Sleep -Milliseconds 400; $champ.SetFocus(); Start-Sleep -Milliseconds 300 }
  # securite : on ne tape que si la fenetre active est WhatsApp ET si le curseur est dans le champ de recherche
  # (jamais dans la zone « Ecrivez un message », ou une faute enverrait un message a quelqu'un)
  $ou = [System.Windows.Automation.AutomationElement]::FocusedElement
  if (-not (Devant-Ok) -or -not $ou -or $ou.Current.Name -notmatch 'Rechercher|Search') { Fin 'ERREUR:fenetre' }
  [System.Windows.Forms.SendKeys]::SendWait(($cible -replace '([+^%~(){}\[\]])', '{$1}'))
  Start-Sleep -Milliseconds 1800     # WhatsApp met un instant a afficher les resultats
  # la premiere ligne de resultat (pas un titre de rubrique)
  $titres = '^(Contacts|Discussions|Chats|Groupes en commun|Groups in common|Messages|Groupes|Groups)$'
  $resultat = Attendre {
    $grille = Tous | Where-Object { (Genre $_) -eq 'DataGrid' -and $_.Current.Name -match 'sultats|results' } | Select-Object -First 1
    if ($grille) { Tous | Where-Object { (Genre $_) -eq 'DataItem' -and $_.Current.Name -and $_.Current.Name -notmatch $titres } | Select-Object -First 1 }
  } 4000
  if (-not $resultat) { Fin 'ERREUR:introuvable' }
  $ligne = Simple $resultat.Current.Name
  if (-not (Cliquer $resultat)) { Fin 'ERREUR:introuvable' }
  # la discussion ouverte doit etre celle de la ligne choisie (et pas l'ancienne, encore affichee un instant)
  $nom = Attendre { $n = Entete; if ($n -and $ligne.Contains((Simple $n))) { $n } } 5000
  if (-not $nom) { Fin 'ERREUR:introuvable' }
  return $nom
}

if ($Mode -eq 'ouvrir') {
  $nom = Ouvrir
  if (-not (Attendre { BoutonAppel } 3000)) { Fin "ERREUR:pas_d_appel:$nom" }
  Fin "CONTACT:$nom"
}

# Mode appeler : la discussion doit etre celle qu'on a annoncee a l'utilisateur
$nom = Entete
if (-not $nom -or (Simple $nom) -ne (Simple $attendu)) { $nom = Ouvrir }
if ((Simple $nom) -ne (Simple $attendu)) { Fin "ERREUR:autre_contact:$nom" }
$bouton = Attendre { BoutonAppel } 4000
if (-not $bouton) { Fin "ERREUR:pas_d_appel:$nom" }
$bouton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Fin "APPEL:$nom"
