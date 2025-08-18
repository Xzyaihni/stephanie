(define (generate-roof)
    (fill-area
        (filled-chunk (tile 'air))
        (make-area
            (make-point 5 4)
            (make-point (- size-x 9) (- size-y 7)))
        (tile 'concrete)))

(define (generate-floor)
    (define this-chunk (filled-chunk (tile 'soil)))
    (fill-area
        this-chunk
	(make-area
	    (make-point 6 5)
	    (make-point (- size-x 11) (- size-y 9)))
	(tile 'concrete)))

(define (generate-ground)
    (define this-chunk (filled-chunk (tile 'grassie)))
    (fill-area
        this-chunk
	(make-area
	    (make-point 6 5)
	    (make-point (- size-x 11) (- size-y 9)))
	(tile 'concrete))
    (put-tile
        this-chunk
        (make-point (/ size-x 2) (- size-y 5))
        (tile 'stairs-down (side-combine side-down rotation))))

(define (generate-walls)
    (if (= height 1)
        (begin
            (define this-chunk (filled-chunk (tile 'air)))
            (rectangle-outline
                this-chunk
                (make-area
                    (make-point 5 4)
                    (make-point (- size-x 9) (- size-y 7)))
                (tile 'concrete))
            (put-tile
                this-chunk
                (make-point (/ size-x 2) (/ size-y 2))
                (single-marker (list 'light 1.3)))
            (let ((doorway-point (make-point (- (/ size-x 2) 1) 4)))
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
                        (single-marker (list 'door side-left 'metal 2))))))
        (begin
            (define this-chunk (filled-chunk (tile 'soil)))
            (rectangle-outline
                this-chunk
                (make-area
                    (make-point 5 4)
                    (make-point (- size-x 9) (- size-y 7)))
                (tile 'concrete))
            (fill-area
                this-chunk
                (make-area
                    (make-point 6 5)
                    (make-point (- size-x 11) (- size-y 9)))
                (tile 'air))
            (put-tile
                this-chunk
                (make-point (/ size-x 2) (/ size-y 2))
                (single-marker (list 'light 1.3)))
            (put-tile
                this-chunk
                (make-point (/ size-x 2) (- size-y 5))
                (tile 'stairs-up (side-combine side-down rotation))))))

(if (= height 2)
    (generate-roof)
    (if (or (= height 1) (= height -1))
        (generate-walls)
        (if (= height 0)
            (generate-ground)
            (generate-floor))))
