#/bin/sh

while getopts l:m:r name
do
    case $name in
        l)
            LED_NO=${OPTARG}
            ;;
        m)
            MESSAGE=${OPTARG}
            ;;
        r)
            RESET=1
            ;;
        ?)
            echo "Usage: $0 [-l no ] [-m message] [server]"
            echo "-l LED no to modify (default 0)"
            echo "-m LED message to send to 'set_led_no',"
            echo "   integers are mapped to valid messages"
            echo "-r reset all LEDs"
            exit 1
            ;;
    esac
done

shift "$((OPTIND-1))"

if ! [ -z "${MESSAGE##*[!0-9]*}" ]; then
    if [ $MESSAGE -eq 0 ]; then
        MESSAGE="ok"
    else
        MESSAGE="error"
    fi
fi

if ! [ -z ${RESET+x} ]; then
    curl -H "content-type: application/json" -X POST -d "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"reset\",\"params\":[]}" $1 -w '\n'   
else
    curl -H "content-type: application/json" -X POST -d "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"set_led_no\",\"params\":[${LED_NO:-0}, \"$MESSAGE\"]}" $1 -w '\n'
fi        
