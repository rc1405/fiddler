#!/usr/bin/bash

lint_failures=`ls -1 tests|grep lint|grep failed`
for i in $lint_failures
do 
    if [ `echo $i|grep lint|wc -l` -gt 0 ]
    then 
        ../target/release/fiddler-cli lint -c tests/$i/input.yaml > /dev/null 
        if [ "$?" -eq 0 ]
        then
            echo lint $i succeeded when it should have failed
            exit 1
        fi
    fi
done

lint_successes=`ls -1 tests|grep lint|grep success`
for i in $lint_successes
do 
    if [ `echo $i|grep lint|wc -l` -gt 0 ]
    then 
        ../target/release/fiddler-cli lint -c tests/$i/input.yaml > /dev/null
        if [ "$?" -ne 0 ]
        then
            echo lint $i failed
            exit 1
        fi
    fi
done

test_failures=`ls -1 tests|grep test|grep failed`
for i in $test_failures
do 
    if [ `echo $i|grep lint|wc -l` -gt 0 ]
    then 
        ../target/release/fiddler-cli test -c tests/$i/input.yaml > /dev/null
        if [ "$?" -eq 0 ]
        then
            echo test $i succeeded when it should have failed
            exit 1
        fi
    fi
done

test_successes=`ls -1 tests|grep test|grep success`
for i in $test_successes
do 
    if [ `echo $i|grep lint|wc -l` -gt 0 ]
    then 
        ../target/release/fiddler-cli test -c tests/$i/input.yaml > /dev/null
        if [ "$?" -ne 0 ]
        then
            echo test $i failed
            exit 1
        fi
    fi
done

echo success