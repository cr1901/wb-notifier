SERVER_ADDR=  # User sets this!

# Quick Notify
qnoti() {
  wbn-client $SERVER_ADDR notify -l ${1-0} -s $? > /dev/null
}

# Quick Acknowledge
qack() {
  if ! [ -z ${1+x} ]; then
    wbn-client $SERVER_ADDR ack -l ${1-0} > /dev/null
  else
    wbn-client $SERVER_ADDR ack > /dev/null
  fi
}
