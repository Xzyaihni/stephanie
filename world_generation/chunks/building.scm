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
            (tile 'stairs-down rotation))))

(define (generate-room)
    (define (residential-building)
        (define this-chunk (filled-chunk (tile 'air)))

        (define (this-tile point tle) (put-tile this-chunk point tle))

	(define (maybe-light point intensity offset)
            (if (stop-between-difficulty 0.1 0.2)
		(combine-markers this-chunk point (list 'light intensity offset))))

        (define (decide-enemy type)
            (if (eq? type 'normal)
                (pick-weighted 'zob 'runner 0.25)
                'bigy))

        (define (maybe-enemy point)
            (if (difficulty-chance 0.5 0.3)
                (combine-markers
                    this-chunk
                    point
                    (list
                        'enemy
                        (decide-enemy
                            (gradient-pick
				'(normal strong)
				difficulty
				0.0
				0.6))))))

        (define wall-material (tile 'concrete))

        (define (door x y side material)
            (this-tile
                (make-point x y)
		(single-marker (list 'door side material 1))))

        (define (room-side flip)
	    (define (add-window y)
		(this-tile
		    (make-point (x-of 1) y)
		    (tile 'glass)))

            (define (x-of x)
                (if flip
                    (- (- size-x 1) x)
                    x))

            (define (area-of a)
                (if flip
                    (let ((start (area-start a)) (size (area-size a)))
                        (make-area
                            (make-point (- (x-of (point-x start)) (- (point-x size) 1)) (point-y start))
                            size))
                    a))

            ((random-choice
                (list
                    (lambda ()
                        (add-window 11)
                        (add-window 12)
                        (this-tile (make-point (x-of 3) 14) (tile 'glass))
                        (this-tile (make-point (x-of 4) 14) (tile 'glass))
                        (door (x-of 6) 7 side-up 'metal)
                        (fill-area
                            this-chunk
                            (area-of (make-area (make-point 3 5) (make-point 3 1)))
                            wall-material)
                        (fill-area
                            this-chunk
                            (area-of (make-area (make-point 2 9) (make-point 4 1)))
                            wall-material)
                        (door (x-of 3) 9 (if flip side-left side-right) 'metal)
                        (maybe-light (make-point (x-of 3) 4) 0.7 '(0.0 0.0 0.0))
                        (maybe-light (make-point (x-of 3) 7) 0.7 (list (if flip -0.5 0.5) 0.0 0.0))
                        (maybe-light (make-point (x-of 3) 12) 0.8 (list (if flip -0.5 0.5) 0.0 0.0))
                        (door (x-of 2) 5 (if flip side-left side-right) 'metal)
                        (maybe-enemy (make-point (x-of (random-integer-between 2 6)) (random-integer-between 6 (- size-y 7)))))
                    (lambda ()
                        (add-window 8)
                        (add-window 9)
                        (add-window 10)
                        (add-window 11)
                        (this-tile (make-point (x-of 3) 14) (tile 'glass))
                        (this-tile (make-point (x-of 4) 14) (tile 'glass))
                        (door (x-of 6) 4 side-up 'metal)
                        (fill-area
                            this-chunk
                            (area-of (make-area (make-point 3 5) (make-point 3 1)))
                            wall-material)
                        (maybe-light (make-point (x-of 3) 3) 0.8 '(0.0 0.0 0.0))
                        (maybe-light (make-point (x-of 4) 9) 1.2 (list (if flip -0.5 0.5) 0.5 0.0))
                        (door (x-of 2) 5 (if flip side-left side-right) 'metal)
                        (maybe-enemy (make-point (x-of (random-integer-between 2 6)) (random-integer-between 6 (- size-y 2)))))
                    (lambda ()
                        (this-tile (make-point (x-of 2) 1) (tile 'glass))
                        (this-tile (make-point (x-of 3) 1) (tile 'glass))
                        (this-tile (make-point (x-of 4) 1) (tile 'glass))
                        (door (x-of 6) 12 side-up 'metal)
			(rectangle-outline
			    this-chunk
			    (area-of
                                (make-area
				    (make-point 1 10)
				    (make-point 4 5)))
			    wall-material)
                        (maybe-light (make-point (x-of 4) 4) 1.2 '(0.0 0.5 0.0))
                        (maybe-light (make-point (x-of 2) 12) 0.5 (list (if flip -0.5 0.5) 0.0 0.0))
                        (maybe-light (make-point (x-of 5) 12) 0.6 '(0.0 0.0 0.0))
                        (door (x-of 4) 12 side-up 'metal)
                        (maybe-enemy (make-point (x-of (random-integer-between 2 5)) (random-integer-between 2 (- size-y 7)))))))))

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

        (define (hallway-enemy x)
            (maybe-enemy (make-point x (random-integer-between 1 (- size-y 2)))))

        (hallway-enemy 7)
        (hallway-enemy 8)

	(maybe-light (make-point 7 4) 0.9 '(0.5 -0.4 0.0))
	(maybe-light (make-point 7 10) 0.9 '(0.5 0.4 0.0))

        (room-side #f)
        (room-side #t)

        this-chunk)

    (define this-chunk (residential-building))

    (let ((x (if (= (remainder height 4) 3) 6 9)))
        (put-tile
            this-chunk
            (make-point x 1)
            (tile 'stairs-up rotation)))

    (if (= height 1)
        ; entrance
        (begin
            (horizontal-line-length
                this-chunk
                (make-point 7 0)
                2
                (tile 'air))
            (if (< (random-float) 0.3)
                (combine-markers
                    this-chunk
                    (make-point (if (random-bool) 0 (- size-x 1)) (random-integer-between 1 (- size-y 1)))
                    (list 'furniture 'crate)))
            (put-tile
                this-chunk
                (make-point 7 0)
                (single-marker (list 'door side-left 'metal 2))))
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
                (tile 'stairs-down rotation)))
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
                        (make-point 6 4))
                    (tile 'concrete))
                (fill-area
                    this-chunk
                    (make-area
                        (make-point 6 1)
                        (make-point 4 2))
                    (tile 'air))
		(put-tile
                    this-chunk
		    (make-point 7 2)
		    (single-marker (list 'light 0.6 '(0.5 0.0 0.0))))
                (put-tile
                    this-chunk
		    (make-point 8 3)
		    (tile 'air))
		(put-tile
                    this-chunk
		    (make-point 9 3)
		    (single-marker (list 'door side-right 'metal 2))))
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
