(define (generate-roof)
    (fill-area
        (filled-chunk (tile 'air))
        (make-area
            (make-point 2 2)
            (make-point (- size-x 4) (- size-y 4)))
        (tile 'concrete)))

(define (generate-floor) (filled-chunk (tile 'concrete)))

(define (generate-ground)
    (define this-chunk (filled-chunk (tile 'air)))
    (fill-area
        this-chunk
        (make-area
            (make-point (/ size-x 2) 0)
            (make-point 1 size-y))
        (tile 'grassie))
    (put-tile
        this-chunk
        (make-point (/ size-x 2) (- size-y 7))
        (tile 'air))
    (put-tile
        this-chunk
        (make-point (/ size-x 2) (- size-y 4))
        (tile 'stairs-down 'Down)))

(define (generate-walls)
    (define this-chunk (filled-chunk (tile 'air)))
    (rectangle-outline
        this-chunk
        (make-area
            (make-point 2 2)
            (make-point (- size-x 4) (- size-y 4)))
        (tile 'concrete))
    (if (= height 1)
        (begin
            (let ((doorway-point (make-point (- (/ size-x 2) 1) 2)))
                (begin
                    (fill-area
                        this-chunk
                        (make-area
                            doorway-point
                            (make-point 2 1))
                        (tile 'air))
                    (put-tile
                        this-chunk
                        doorway-point
                        (tile 'metal-door-wide)))))
        (put-tile
            this-chunk
            (make-point (/ size-x 2) (- size-y 4))
            (tile 'stairs-up 'Down))))

(if (= height 2)
    (generate-roof)
    (if (or (= height 1) (= height -1))
        (generate-walls)
        (if (= height 0)
            (generate-ground)
            (generate-floor))))
