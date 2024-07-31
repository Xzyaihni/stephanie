(define (generate-ground)
    (define this-chunk
        (fill-area
            (filled-chunk (tile 'concrete))
            (make-area
                (make-point 2 2)
                (make-point (- size-x 4) (- size-y 4)))
            (tile 'wood)))

    (fill-area
        this-chunk
        (make-area
            (make-point 6 2)
            (make-point 4 2))
        (tile 'concrete)))

(define (generate-floor)
    (define this-chunk
        (fill-area
            (filled-chunk (tile 'air))
            (make-area
                (make-point 1 1)
                (make-point (- size-x 2) (- size-y 2)))
            (tile 'wood)))

    (fill-area
        this-chunk
        (make-area
            (make-point 5 0)
            (make-point 6 3))
        (tile 'concrete))

    (let ((x (if (= (remainder height 4) 0) 6 9)))
        (put-tile
            this-chunk
            (make-point x 1)
            (tile 'stairs-down))))

(define (generate-room)
    (define (residential-building)
        (define this-chunk (filled-chunk (tile 'air)))

        (define (this-tile point tile) (put-tile this-chunk point tile))

        (define wall-material (tile 'concrete))

        ; outer walls
        (rectangle-outline
            this-chunk
            (make-area
                (make-point 1 1)
                (make-point (- size-x 2) (- size-y 2)))
            wall-material)

        ; hallway
        (fill-area
            this-chunk
            (make-area
                (make-point 6 0)
                (make-point 4 (- size-x 1)))
            wall-material)

        ; stairwell
        (fill-area
            this-chunk
            (make-area
                (make-point 5 0)
                (make-point 6 4))
            wall-material)

        (fill-area
            this-chunk
            (make-area
                (make-point 6 1)
                (make-point 4 2))
            (tile 'air))

        (fill-area
            this-chunk
            (make-area
                (make-point 7 1)
                (make-point 2 (- size-y 3)))
            (tile 'air))

        (define (door x)
            (this-tile
                (make-point x 12)
                (tile 'air)))

        (door 6)
        (door 9)

        (define (add-windows x)
            (this-tile
                (make-point x 3)
                (tile 'glass))
            (this-tile
                (make-point x (- size-y 4))
                (tile 'glass)))

        (add-windows 1)
        (add-windows (- size-x 2))

        this-chunk)

    (define this-chunk (residential-building))

    (let ((x (if (= (remainder height 4) 3) 6 9)))
        (put-tile
            this-chunk
            (make-point x 1)
            (tile 'stairs-up)))

    (if (= height 1)
        ; entrance
        (horizontal-line-length
            this-chunk
            (make-point 7 0)
            2
            (tile 'air))
        this-chunk))

(define (generate-roof level)
    (define this-chunk (filled-chunk (tile 'air)))
    (if (= level 0)
        (begin
            (define this-chunk
                (fill-area
                    this-chunk
                    (make-area
                        (make-point 1 1)
                        (make-point (- size-x 2) (- size-y 2)))
                    (tile 'concrete)))

            (fill-area
                this-chunk
                (make-area
                    (make-point 5 0)
                    (make-point 6 1))
                (tile 'concrete))

            (put-tile
                this-chunk
                (make-point 6 1)
                (tile 'stairs-down)))
        (if (= level 1)
            (begin
                (rectangle-fence
                    this-chunk
                    (make-area
                        (make-point 1 1)
                        (make-point (- size-x 2) (- size-y 2)))
                    'concrete-fence
                    'concrete-fence-l)
                (fill-area
                    this-chunk
                    (make-area
                        (make-point 5 0)
                        (make-point 6 3))
                    (tile 'concrete))
                (fill-area
                    this-chunk
                    (make-area
                        (make-point 6 1)
                        (make-point 4 2))
                    (tile 'air)))
            (fill-area
                this-chunk
                (make-area
                    (make-point 5 0)
                    (make-point 6 3))
                (tile 'concrete)))))

(define roof-start (- building-height 3))

(if (= height 0)
    (generate-ground)
    (if (>= height roof-start)
        (generate-roof (- height roof-start))
        (if (= (remainder height 2) 0)
            (generate-floor)
            (generate-room))))
