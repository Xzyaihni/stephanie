(if (= height 1)
    (begin
        (define this-chunk (filled-chunk (tile 'air)))

        (make-park-walls this-chunk)

        (make-park-grass this-chunk (make-area (make-point 1 1) (make-point (- size-x 2) (- size-y 2))))

        this-chunk)
    (filled-chunk (tile 'grassie)))
