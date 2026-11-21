# Appel vocal dans l'appli WhatsApp du PC, pilotee comme le ferait une personne (recherche, Entree, bouton).
# Lance par JarvisBarre (pouvoirs.rs). La personne est donnee par la variable JARVIS_WA_CIBLE (nom ou numero).
#   -Mode ouvrir  : cherche la personne et ouvre sa discussion. Ecrit « CONTACT:<nom affiche> ».
#   -Mode liste / lire / envoyer : les discussions, les derniers messages, ou envoyer JARVIS_WA_TEXTE.
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
  public static int Distance(string a, string b) {
    var d = new int[a.Length + 1, b.Length + 1];
    for (int i = 0; i <= a.Length; i++) d[i, 0] = i;
    for (int j = 0; j <= b.Length; j++) d[0, j] = j;
    for (int i = 1; i <= a.Length; i++)
      for (int j = 1; j <= b.Length; j++)
        d[i, j] = Math.Min(Math.Min(d[i - 1, j] + 1, d[i, j - 1] + 1), d[i - 1, j - 1] + (a[i - 1] == b[j - 1] ? 0 : 1));
    return d[a.Length, b.Length];
  }
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
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr p);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent, IntPtr h);
  // la fenetre active est WhatsApp : la fenetre elle-meme, sa page web, ou une fenetre qui leur appartient
  public static bool EstDevant(IntPtr main, IntPtr web) {
    IntPtr f = GetForegroundWindow();
    if (f == IntPtr.Zero) return false;
    IntPtr r = GetAncestor(f, 3);
    return f == main || f == web || r == main || r == web || IsChild(main, f) || IsChild(web, f);
  }
  public static string Titre(IntPtr h) { var t = new StringBuilder(256); GetWindowText(GetAncestor(h, 3), t, 256); return t.ToString(); }
  public static void Devant(IntPtr h) {
    if (IsIconic(h)) ShowWindow(h, 9);
    // on se branche un instant sur la fenetre active : Windows accepte alors de mettre WhatsApp devant
    uint actif = GetWindowThreadProcessId(GetForegroundWindow(), IntPtr.Zero), moi = GetCurrentThreadId();
    bool lie = actif != moi && AttachThreadInput(moi, actif, true);
    keybd_event(0x12, 0, 0, UIntPtr.Zero); keybd_event(0x12, 0, 2, UIntPtr.Zero);   // Alt
    BringWindowToTop(h);
    SetForegroundWindow(h);
    if (lie) AttachThreadInput(moi, actif, false);
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
  if ($b -and $b.Current.Name -match '^(Infos du contact|Contact info|Infos du groupe|Group info)\s+(.+)$') { return $Matches[2].Trim() }
  return $null
}
function BoutonAppel { Tous | Where-Object { (Genre $_) -eq 'Button' -and $_.Current.Name -match '^(Appel vocal|Voice call)$' } | Select-Object -First 1 }
# FormKC : les lettres décorées (𝓛𝓾𝓬𝓪𝓼, ＬＵＣＡＳ) redeviennent des lettres normales ; on enlève accents, emojis, symboles
function Mots($t) { (("$t".Normalize([Text.NormalizationForm]::FormKC).Normalize([Text.NormalizationForm]::FormD) -replace '\p{Mn}', '').ToLower() -replace '[^a-z0-9+ ]', ' ' -replace '\s+', ' ').Trim() }
function Simple($t) { (Mots $t) -replace ' ', '' }
function Distance([string]$a, [string]$b) { return [JarvisWa]::Distance($a, $b) }   # lettres à changer pour passer de a à b
function Ressemblance($dit, $nom) {   # 0 à 100 : « mamann » ~ « Maman 💕 », « lucas » ~ « 𝓛𝓾𝓬𝓪𝓼 🔥 »
  $a = Mots $dit; $b = Mots $nom
  if (-not $a -or -not $b) { return 0 }
  if ($a -eq $b) { return 100 }
  if ($b.StartsWith("$a ") -or (" $b ").Contains(" $a ")) { return 88 }
  $sa = $a -replace ' ', ''; $meilleur = 0.0
  foreach ($w in @(($b -replace ' ', '')) + $b.Split(' ')) {
    if ($w.Length -lt 2) { continue }
    $r = 1 - (Distance $sa $w) / [Math]::Max($sa.Length, $w.Length)
    if ($r -gt $meilleur) { $meilleur = $r }
  }
  return [int](100 * $meilleur)
}
function NomDeLigne($t) {   # « Maman 💕 18:42 » -> « Maman 💕 » (le titre d'une discussion, sans son heure)
  return ("$t" -replace '\s+(\d{1,2}:\d{2}|\d{1,2}/\d{1,2}/\d{2,4}|hier|aujourd.hui|yesterday|lundi|mardi|mercredi|jeudi|vendredi|samedi|dimanche)\s*$', '').Trim()
}

function Devant-Ok { return [JarvisWa]::EstDevant($main, $web) }
function ClavierDans($x) {   # WhatsApp est la fenetre active ET le clavier est EXACTEMENT dans cet element
  if (-not (Devant-Ok)) {
    # Windows refuse parfois de passer WhatsApp devant : le lien « whatsapp: » (ouvert par Windows) y arrive
    try { Start-Process 'whatsapp:' } catch { }
    Start-Sleep -Milliseconds 900
    $x.SetFocus(); Start-Sleep -Milliseconds 300
  }
  $ou = [System.Windows.Automation.AutomationElement]::FocusedElement
  return ((Devant-Ok) -and $ou -and [System.Windows.Automation.Automation]::Compare($ou, $x))
}

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

function OuvrirDepuisListe {
  # Sans rien taper : le titre de discussion qui ressemble le plus à ce qui a été dit, dans la colonne de gauche
  $titres = Tous | Where-Object { (Genre $_) -eq 'DataItem' -and $_.Current.Name -and $_.Current.BoundingRectangle.Height -gt 0 -and $_.Current.BoundingRectangle.Height -lt 45 }
  $choisi = $null; $nomChoisi = ''; $note = 0
  foreach ($x in $titres) {
    $nom = NomDeLigne $x.Current.Name
    $s = Ressemblance $cible $nom
    if ($s -gt $note) { $note = $s; $choisi = $x; $nomChoisi = $nom }
  }
  if ($note -lt 72) { return $null }
  if (-not (Devant-Ok)) { try { Start-Process 'whatsapp:' } catch { }; Start-Sleep -Milliseconds 900 }
  if (-not (Devant-Ok)) { return $null }   # jamais de clic si WhatsApp n'est pas devant
  $r = $choisi.Current.BoundingRectangle
  [JarvisWa]::Clic([int]($r.Left + $r.Width / 2), [int]($r.Top + $r.Height / 2))
  $voulu = Simple $nomChoisi
  return (Attendre { $e = Entete; if ($e -and ((Simple $e) -eq $voulu -or $voulu.Contains((Simple $e)) -or (Simple $e).Contains($voulu))) { $e } } 4000)
}

function Ouvrir {
  $nom = OuvrirDepuisListe
  if ($nom) { return $nom }
  # on repart d'une recherche vide (le champ ne se montre plus quand il contient deja du texte)
  $vider = Tous | Where-Object { (Genre $_) -eq 'Button' -and $_.Current.Name -match '^(End icon button|Annuler la recherche|Cancel search)$' } | Select-Object -First 1
  if ($vider) { [void](Cliquer $vider); Start-Sleep -Milliseconds 600 }
  $champ = Attendre { Tous | Where-Object { (Genre $_) -eq 'Edit' -and $_.Current.Name -match 'Rechercher|Search' } | Select-Object -First 1 } 6000
  if (-not $champ) { Fin 'ERREUR:recherche' }
  $champ.SetFocus(); Start-Sleep -Milliseconds 300
  if (-not (Devant-Ok)) { [JarvisWa]::Devant($main); Start-Sleep -Milliseconds 400; $champ.SetFocus(); Start-Sleep -Milliseconds 300 }
  # securite : on ne tape que si la fenetre active est WhatsApp ET si le curseur est dans le champ de recherche
  # (jamais dans la zone « Ecrivez un message », ou une faute enverrait un message a quelqu'un)
  # (la page de WhatsApp est souvent une fenetre web sans titre : on verifie donc l'element qui a le clavier)
  if (-not (ClavierDans $champ)) { Fin 'ERREUR:fenetre' }
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


# ── Lire et écrire (modes liste, lire, envoyer) ──
$ecrire = 'crire un message|Saisissez|Tapez un message|Entrez un message|Type a message'
function ZoneMessage { Tous | Where-Object { @('Edit', 'Document') -contains (Genre $_) -and $_.Current.Name -match $ecrire -and $_.Current.BoundingRectangle.Left -gt 300 } | Select-Object -First 1 }
function Liste {   # les discussions de la colonne de gauche : nom, heure, dernier message (et les non lus)
  $vus = @{}
  $n = 0
  foreach ($x in (Tous | Where-Object { (Genre $_) -eq 'DataItem' -and $_.Current.BoundingRectangle.Height -gt 60 -and $_.Current.Name })) {
    $t = ($x.Current.Name -replace '\s+', ' ').Trim()
    if (-not $vus.ContainsKey($t)) { $vus[$t] = 1; Write-Output "DISC:$t"; $n++; if ($n -ge 12) { break } }
  }
}
function Messages {   # la discussion ouverte : les textes entre l'en-tete et la zone d'ecriture, de haut en bas
  $grille = Tous | Where-Object { (Genre $_) -eq 'DataGrid' } | Select-Object -First 1
  $gauche = if ($grille) { $grille.Current.BoundingRectangle.Right } else { 330 }
  $zone = ZoneMessage
  $bas = if ($zone) { $zone.Current.BoundingRectangle.Top } else { 100000 }
  $textes = New-Object System.Collections.Generic.List[string]
  $morceaux = Tous | Where-Object {
    $r = $_.Current.BoundingRectangle
    $r.Left -gt $gauche -and $r.Top -gt 90 -and $r.Bottom -le $bas -and $_.Current.Name -and @('Text', 'ListItem', 'Group', 'DataItem') -contains (Genre $_)
  } | Sort-Object { $_.Current.BoundingRectangle.Top }
  foreach ($x in $morceaux) {
    $t = ($x.Current.Name -replace '\s+', ' ').Trim()
    if ($t.Length -gt 1 -and $t -notmatch '^(Infos du contact|Contact info|Appel vocal|Voice call|Rechercher|Search)') { $textes.Add($t) }
  }
  # un message entier contient ses morceaux : on ne garde pas les morceaux
  $gardes = $textes | Where-Object { $t = $_; -not ($textes | Where-Object { $_ -ne $t -and $_.Contains($t) }) } | Select-Object -Unique
  $gardes | Select-Object -Last 20 | ForEach-Object { Write-Output "MSG:$_" }
}

if ($Mode -eq 'liste') { Liste; Fin 'FIN' }
if ($Mode -eq 'lire') {
  $nom = Ouvrir
  Start-Sleep -Milliseconds 1200
  Messages
  Fin "CONTACT:$nom"
}
if ($Mode -eq 'envoyer') {
  # la discussion doit etre celle annoncee a l'utilisateur (son « oui » vaut pour cette personne seulement)
  $nom = Entete
  if (-not $nom -or (Simple $nom) -ne (Simple $attendu)) { $nom = Ouvrir }
  if ((Simple $nom) -ne (Simple $attendu)) { Fin "ERREUR:autre_contact:$nom" }
  $zone = Attendre { ZoneMessage } 4000
  if (-not $zone) { Fin "ERREUR:fenetre" }
  $zone.SetFocus(); Start-Sleep -Milliseconds 300
  if (-not (Devant-Ok)) { [JarvisWa]::Devant($main); Start-Sleep -Milliseconds 400; $zone.SetFocus(); Start-Sleep -Milliseconds 300 }
  if (-not (ClavierDans $zone)) { Fin 'ERREUR:fenetre' }
  [System.Windows.Forms.SendKeys]::SendWait(("$env:JARVIS_WA_TEXTE" -replace '([+^%~(){}\[\]])', '{$1}'))
  Start-Sleep -Milliseconds 400
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Fin "ENVOYE:$nom"
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
