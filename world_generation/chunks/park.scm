(if (= height 1)
    (begin
        (define this-chunk (filled-chunk (tile 'air)))

        (make-park-walls this-chunk)

        this-chunk)
    (filled-chunk (tile 'grassie)))
