complete -c ga4 -f
complete -c ga4 -n '__fish_use_subcommand' -a auth -d 'Manage Google authorization'
complete -c ga4 -n '__fish_use_subcommand' -a properties -d 'List accessible GA4 properties'
complete -c ga4 -n '__fish_use_subcommand' -a export -d 'Export an Overview report'
complete -c ga4 -n '__fish_use_subcommand' -a cache -d 'Manage report cache'
complete -c ga4 -n '__fish_use_subcommand' -a doctor -d 'Show diagnostics'
complete -c ga4 -n '__fish_seen_subcommand_from auth' -a 'login status logout'
complete -c ga4 -n '__fish_seen_subcommand_from cache' -a clear
complete -c ga4 -n '__fish_seen_subcommand_from export' -l format -a 'csv json'
