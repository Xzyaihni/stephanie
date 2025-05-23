(define (generate-roof)
    (fill-area
        (filled-chunk (tile 'air))
        (make-area
            (make-point 2 2)
            (make-point (- size-x 4) (- size-y 4)))
        (tile 'concrete)))

(define (generate-floor) (filled-chunk (tile 'concrete)))

(define (generate-ground)
    (define this-chunk (filled-chunk (tile 'concrete)))
    (put-tile
        this-chunk
        (make-point (/ size-x 2) (- size-y 4))
        (tile 'stairs-down 'down)))

(define (generate-walls)
    (if (= height 1)
        (begin
            (define this-chunk (filled-chunk (tile 'air)))
            (rectangle-outline
                this-chunk
                (make-area
                    (make-point 2 2)
                    (make-point (- size-x 4) (- size-y 4)))
                (tile 'concrete))
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
                        (single-marker (list 'door 'left 'metal 2))))))
        (begin
            (define this-chunk (filled-chunk (tile 'concrete)))
            (fill-area
                this-chunk
                (make-area
                    (make-point 3 3)
                    (make-point (- size-x 6) (- size-y 6)))
                (tile 'air))
            (put-tile
                this-chunk
                (make-point (/ size-x 2) (- size-y 4))
                (tile 'stairs-up 'down)))))

(if (= height 2)
    (generate-roof)
    (if (or (= height 1) (= height -1))
        (generate-walls)
        (if (= height 0)
            (generate-ground)
            (generate-floor))))
