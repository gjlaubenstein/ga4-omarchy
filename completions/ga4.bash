_ga4() {
  local cur prev
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  case "$prev" in
    ga4) COMPREPLY=( $(compgen -W "auth properties export cache doctor help" -- "$cur") );;
    auth) COMPREPLY=( $(compgen -W "login status logout" -- "$cur") );;
    cache) COMPREPLY=( $(compgen -W "clear" -- "$cur") );;
    --format) COMPREPLY=( $(compgen -W "csv json" -- "$cur") );;
    *) COMPREPLY=( $(compgen -W "--json --client-secret --property --start --end --format --output --comparison --help --version" -- "$cur") );;
  esac
}
complete -F _ga4 ga4
