#!/bin/sh
# Faux Claude CLI pour les tests (l'environnement de l'enfant est vidé : tout passe par les
# arguments et le jeton).
# - `setup-token` : imprime une URL entourée de codes ANSI, lit le code, imprime un jeton
#   (code « mauvais » : échec).
# - `-p ...` : réponse JSON qui renvoie les arguments et l'environnement reçus ;
#   jeton contenant « mauvais » : réponse en erreur.
if [ "$1" = "setup-token" ]; then
  printf '\033[1mOuvrez ce lien :\033[0m \033[4mhttps://claude.ai/oauth/authorize?code=true&state=abc123\033[0m\r\n'
  printf 'Paste code here if prompted > '
  read code
  if [ "$code" = "mauvais" ]; then printf 'Invalid code\r\n'; exit 1; fi
  printf '\r\n\033[32mLong-lived authentication token created:\033[0m\r\n'
  printf 'sk-ant-oat01-test_123\r\n'
  exit 0
fi
case "$CLAUDE_CODE_OAUTH_TOKEN" in
  *mauvais*) echo '{"type":"result","is_error":true,"result":"Invalid bearer token"}'; exit 1 ;;
esac
printf '{"type":"result","is_error":false,"result":"OK","args":"%s","home":"%s","autoupdate":"%s"}\n' "$*" "$HOME" "$DISABLE_AUTOUPDATER"
