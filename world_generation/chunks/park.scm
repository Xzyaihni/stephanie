(if (= height 1)
    (begin
        (define this-chunk (filled-chunk (tile 'air)))

        (make-park-walls this-chunk)

        (make-park-grass this-chunk (make-area (make-point 2 2) (make-point (- size-x 4) (- size-y 4))))

        this-chunk)
    (filled-chunk (tile 'grassie)))
