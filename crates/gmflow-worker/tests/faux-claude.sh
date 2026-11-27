#!/bin/sh
# Faux Claude CLI pour les tests (l'environnement de l'enfant est vidé : tout passe par les
# arguments et le jeton).
# - `setup-token` : imprime une URL entourée de codes ANSI, lit le code, imprime un jeton
#   (code « mauvais » : échec ; code « refuse » : message d'erreur OAuth du vrai CLI, espaces en
#   déplacements du curseur, et le CLI reste ouvert).
# - `-p ...` : réponse JSON qui renvoie les arguments et l'environnement reçus ;
#   jeton contenant « mauvais » : réponse en erreur.
if [ "$1" = "setup-token" ]; then
  printf '\033[1mOuvrez ce lien :\033[0m \033[4mhttps://claude.com/cai/oauth/authorize?code=true&state=abc123\033[0m\r\n'
  printf "Browser didn't open? Use the url below to sign in\r\n"
  printf 'Paste code here if prompted > '
  read code
  if [ "$code" = "mauvais" ]; then printf 'Invalid code\r\n'; exit 1; fi
  if [ "$code" = "refuse" ]; then
    printf 'OAuth\033[1Cerror:\033[1CThat\033[1Ccode\033[1Cwasn'"'"'t accepted. Sign in again to get a new code.Press Enter to retry.\r\n'
    read encore
    exit 0
  fi
  printf '\r\n\033[32mLong-lived authentication token created:\033[0m\r\n'
  printf 'sk-ant-oat01-test_123\r\n'
  exit 0
fi
# Lecture par tranches (`FAUX_TRANCHES` = dossier, posé par un script d'appel du test) :
# - appel d'en-tête (schéma avec `nombre_lignes_annonce`) : rend `entete.json` ;
# - tranche de lignes (« Cette partie couvre les pages N à M ») : recopie les lignes « REF… »
#   du texte de la tranche (« désignation quantité prix montant »), plus la première ligne du
#   texte « APRÈS » (doublon de jonction que le code doit retirer). Une ligne « N modèle » ou
#   « N * » du fichier `echec` fait échouer la tranche qui commence à la page N.
# Chaque appel est noté dans `journal` (« entete haiku », « tranche 4 sonnet pdf » : « pdf »
# quand le fichier `document.pdf` est dans le dossier de travail).
if [ -n "$FAUX_TRANCHES" ]; then
  prompt=''; schema=''; modele=''; prec=''
  for a in "$@"; do
    case "$prec" in
      -p) prompt="$a" ;;
      --json-schema) schema="$a" ;;
      --model) modele="$a" ;;
    esac
    prec="$a"
  done
  pdf=''; [ -f document.pdf ] && pdf=' pdf'
  case "$schema" in
    *nombre_lignes_annonce*)
      echo "entete $modele$pdf" >> "$FAUX_TRANCHES/journal"
      printf '{"type":"result","is_error":false,"result":"","total_cost_usd":0.002,"structured_output":%s}\n' "$(cat "$FAUX_TRANCHES/entete.json")"
      exit 0 ;;
  esac
  debut=$(printf '%s\n' "$prompt" | sed -nE 's/.*Cette partie couvre (la page|les pages) ([0-9]+).*/\2/p' | head -n 1)
  if [ -n "$debut" ]; then
    echo "tranche $debut $modele$pdf" >> "$FAUX_TRANCHES/journal"
    if [ -f "$FAUX_TRANCHES/echec" ] && grep -qxE "$debut (\*|$modele)" "$FAUX_TRANCHES/echec"; then
      echo '{"type":"result","is_error":false,"result":"Je n arrive pas a lire ces pages."}'
      exit 0
    fi
    printf '%s\n' "$prompt" | awk '
      function nombre(x) { gsub(",", ".", x); return x + 0 }
      function ligne(   d, i) {
        d = $2; for (i = 3; i <= NF - 3; i++) d = d " " $i
        printf "%s{\"reference\":\"%s\",\"designation\":\"%s\",\"quantite\":%.4f,\"prix_unitaire_ht\":%.4f,\"montant_ht\":%.4f}", sep, $1, d, nombre($(NF-2)), nombre($(NF-1)), nombre($NF)
        sep = ","
      }
      BEGIN { mode = ""; sep = ""; printf "{\"type\":\"result\",\"is_error\":false,\"result\":\"\",\"total_cost_usd\":0.001,\"structured_output\":{\"lignes\":[" }
      /^AVANT / { mode = "avant"; next }
      /^Texte de / { mode = "texte"; next }
      /^APR/ { mode = "apres"; next }
      /^>>>$/ { mode = ""; next }
      /^REF[0-9]+ / { if (mode == "texte") ligne(); else if (mode == "apres" && !double) { ligne(); double = 1 } }
      END { print "]}}" }'
    exit 0
  fi
fi
case "$CLAUDE_CODE_OAUTH_TOKEN" in
  *mauvais*) echo '{"type":"result","is_error":true,"result":"Invalid bearer token"}'; exit 1 ;;
esac
printf '{"type":"result","is_error":false,"result":"OK","args":"%s","home":"%s","autoupdate":"%s"}\n' "$*" "$HOME" "$DISABLE_AUTOUPDATER"
