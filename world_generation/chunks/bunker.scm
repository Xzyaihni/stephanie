(define (generate-roof)
    (fill-area
        (filled-chunk (tile 'air))
        (make-area
            (make-point 6 8)
            (make-point 6 6))
        (tile 'concrete)))

(define (generate-floor)
    (define this-chunk (filled-chunk (tile 'soil)))
    (fill-area
        this-chunk
        (make-area
            (make-point 5 4)
            (make-point (- size-x 9) (- size-y 7)))
        (tile 'concrete)))

(define (generate-ground)
    (define this-chunk (filled-chunk (tile 'grassie)))
    (fill-area
        this-chunk
        (make-area
            (make-point 6 8)
            (make-point 6 6))
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
                    (make-point 6 8)
                    (make-point 6 6))
                (tile 'concrete))
            (put-tile
                this-chunk
                (make-point (/ size-x 2) 10)
                (single-marker (list 'light 1.3)))
            (let ((doorway-point (make-point (/ size-x 2) 8)))
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
            (define (place-furniture point name side)
                (combine-markers this-chunk point (list 'furniture name side)))
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
            (place-furniture (make-point 6 6) 'wood_table side-left)
            (place-furniture (make-point 7 5) 'wood_chair side-up)
            (place-furniture (make-point 6 5) 'wood_chair side-up)
            (place-furniture (make-point 7 7) 'wood_chair side-down)
            (place-furniture (make-point 6 7) 'wood_chair side-down)
            (place-furniture (make-point 10 5) 'bed side-up)
            (place-furniture (make-point 10 9) 'sink side-right)
            (if (> difficulty 0.0)
                (combine-markers this-chunk (make-point 8 6) '(enemy me)))
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
