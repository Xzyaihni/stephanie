(define (generate-chunk middle-position part)

(define building-height (assq 'building-height (chunk-tags-at middle-position)))

(if (>= height building-height) (filled-chunk (tile 'air))
(begin

(define big-size-x (* size-x 3))
(define big-size-y (* size-y 3))

(define in-big-chunk-pos
    (point-zip-map
        (make-point size-x size-y)
        (cond
            ((eq? part 'bl) (make-point 0 2))
            ((eq? part 'b) (make-point 1 2))
            ((eq? part 'br) (make-point 2 2))
            ((eq? part 'l) (make-point 0 1))
            ((eq? part 'm) (make-point 1 1))
            ((eq? part 'r) (make-point 2 1))
            ((eq? part 'tl) (make-point 0 0))
            ((eq? part 't) (make-point 1 0))
            (else (make-point 2 0)))
        (lambda (x y) (* x y))))

(define (big-query this-chunk pos on-success on-fail)
    (let (
            (scaled-start (point-sub pos in-big-chunk-pos))
            (clip-check (lambda (v s) (and (> v -1) (< v s)))))
        (if (and
                (clip-check (point-x scaled-start) size-x)
                (clip-check (point-y scaled-start) size-y))
            (on-success scaled-start)
            (on-fail))))

(define (big-combine-markers this-chunk pos marker)
    (big-query
        this-chunk
        pos
        (lambda (scaled-start) (combine-markers this-chunk scaled-start marker))
        (lambda () this-chunk)))

;(define (big-get-tile this-chunk pos)
;    (big-query
;        this-chunk
;        pos
;        (lambda (scaled-start) (get-tile this-chunk scaled-start))
;        (lambda () '())))

(define (big-put-tile this-chunk pos fill-tile)
    (big-query
        this-chunk
        pos
        (lambda (scaled-start) (put-tile this-chunk scaled-start fill-tile))
        (lambda () this-chunk)))

(define (big-fill-area this-chunk area fill-tile)
    (let ((scaled-start (point-sub (area-start area) in-big-chunk-pos)))
        (let (
                (scaled-end (point-zip-map (point-add scaled-start (area-size area)) (make-point size-x size-y) (lambda (x y) (min x y))))
                (clipped-start (point-map scaled-start (lambda (x) (max x 0)))))
            (let ((clipped-size (point-sub scaled-end clipped-start)))
                (if (and
                        (and (> (point-x scaled-end) 0) (< (point-x scaled-start) size-x))
                        (and (> (point-y scaled-end) 0) (< (point-y scaled-start) size-y)))
                    (fill-area this-chunk (make-area clipped-start clipped-size) fill-tile)
                    this-chunk)))))

(define wall-tile (tile 'concrete))

(define (put-outer-walls this-chunk)
    (big-fill-area
        (big-fill-area
            (big-fill-area
                (big-fill-area
                    (big-fill-area
                        (big-fill-area
                            this-chunk
                            (make-area (make-point 1 1) (make-point 7 1))
                            wall-tile)
                        (make-area (make-point 16 1) (make-point 7 1))
                        wall-tile)
                    (make-area (make-point 1 2) (make-point 1 21))
                    wall-tile)
                (make-area (make-point 22 2) (make-point 1 21))
                wall-tile)
            (make-area (make-point 2 22) (make-point 20 1))
            wall-tile)
        (make-area (make-point 7 0) (make-point 10 1))
        wall-tile))

(define (put-floor this-chunk)
    (big-fill-area
        (big-fill-area
            (put-outer-walls this-chunk)
            (make-area (make-point 2 2) (make-point (- big-size-x 4) (- big-size-y 4)))
            (tile 'wood))
        (make-area (make-point 8 1) (make-point size-x 4))
        (tile 'concrete)))

(define roof-start (- building-height 4))

(cond
    ((> height roof-start)
        (cond
            ((= height (+ roof-start 1))
                (define this-chunk
                    (big-fill-area
                        (big-fill-area
                            (filled-chunk (tile 'air))
                            (make-area (make-point 1 1) (make-point 22 22))
                            (tile 'concrete))
                        (make-area (make-point 7 0) (make-point 10 1))
                        (tile 'concrete)))
                (big-put-tile
                    this-chunk
                    (make-point 9 2)
                    (tile 'stairs-down rotation))
                this-chunk)
            ((= height (+ roof-start 2))
                (define this-chunk (filled-chunk (tile 'air)))
                (define fence 'concrete-fence)
                (let
                    (
                        (locked-rotation-a (cond ((= rotation side-right) side-down) ((= rotation side-left) side-up) (else rotation)))
                        (locked-rotation-b (cond ((= rotation side-right) side-up) ((= rotation side-left) side-down) (else rotation))))
                    (begin
                        (big-fill-area this-chunk (make-point (make-point 1 2) (make-point 1 20)) (tile fence (side-combine locked-rotation-b side-up)))
                        (big-fill-area this-chunk (make-point (make-point 17 1) (make-point 5 1)) (tile fence (side-combine locked-rotation-a side-up)))
                        (big-fill-area this-chunk (make-point (make-point 2 1) (make-point 5 1)) (tile fence (side-combine locked-rotation-a side-up)))
                        (big-fill-area this-chunk (make-point (make-point 2 22) (make-point 20 1)) (tile fence (side-combine locked-rotation-a side-down)))
                        (big-fill-area this-chunk (make-point (make-point 22 2) (make-point 1 20)) (tile fence (side-combine locked-rotation-b side-down)))))
                (big-put-tile this-chunk (make-point 22 22) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 1) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 22) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 22 1) (tile 'concrete-fence))
                (big-fill-area this-chunk (make-point (make-point 7 0) (make-point 10 1)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 7 1) (make-point 1 4)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 16 1) (make-point 1 4)) wall-tile)
                (big-fill-area this-chunk (make-point (make-point 8 4) (make-point 6 1)) wall-tile)
                (big-put-tile this-chunk (make-point 9 2) (single-marker (list 'light 0.7 '(0.0 0.0 0.0))))
                (big-put-tile this-chunk (make-point 14 2) (single-marker (list 'light 0.7 '(0.5 0.0 0.0))))
                (big-put-tile this-chunk (make-point 14 4) (single-marker (list 'door side-left 'metal 2))))
            ((= height (+ roof-start 3))
                (big-fill-area (filled-chunk (tile 'air)) (make-point (make-point 7 0) (make-point 10 5)) wall-tile))))
    ((= height 0)
        (put-floor (filled-chunk (tile 'concrete-path))))
    ((= (remainder height 2) 0) (begin
        (define this-chunk (put-floor (filled-chunk (tile 'air))))
        (let ((x (if (= (remainder height 4) 0) 9 14)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-down rotation)))
        this-chunk))
    (else (begin
        (define furnitures-seed (seed-with (seed-with (assq 'building-seed (chunk-tags-at middle-position)) height) 2222))
        (define this-chunk (filled-chunk (tile 'air)))
        (define (try-put-furniture pos t)
            (big-combine-markers this-chunk pos t))
        (define (generate-room-with-furniture room-seed wall-areas furnitures)
            (if (null? furnitures)
                '()
                (let
                    (
                        (wall-areas-length (length wall-areas))
                        (total-area (fold + 0 (map (lambda (wall-area) (area-area (cdr wall-area))) wall-areas))))
                    (let ((selected-area-index (random-integer-seeded (seed-with room-seed 123) wall-areas-length)))
                        (let
                            (
                                (inside-index
                                    (random-integer-seeded (seed-with room-seed 2) (area-area (cdr (list-ref wall-areas selected-area-index))))))
                            (loop
                                (lambda (acc)
                                    (let
                                        (
                                            (inside-index (list-ref acc 0))
                                            (selected-area-index (list-ref acc 1))
                                            (furnitures (list-ref acc 2)))
                                        (let ((selected-area (list-ref wall-areas selected-area-index)))
                                            (let
                                                ((current-area (cdr selected-area)))
                                                (let
                                                    (
                                                        (place-success
                                                            ((car furnitures)
                                                                inside-index
                                                                (car selected-area)
                                                                current-area)))
                                                    (let ((furnitures-tail (if place-success (cdr furnitures) furnitures)))
                                                        (if (null? furnitures-tail)
                                                            '()
                                                            (if (= (+ inside-index 1) (area-area current-area))
                                                                (list
                                                                    0
                                                                    (if (= (+ selected-area-index 1) wall-areas-length) 0 (+ selected-area-index 1))
                                                                    furnitures-tail)
                                                                (list
                                                                    (+ inside-index 1)
                                                                    selected-area-index
                                                                    furnitures-tail)))))))))
                                (list inside-index selected-area-index furnitures)))))))
        (define (generate-main-room room-seed wall-areas middle-area)
            (define skip-tracker (cons 'tracker 0))
            (define (skip-one)
                (if (= (cdr skip-tracker) 0)
                    (begin (set-cdr! skip-tracker 1) #f)
                    #t))
            (define counter (cons 'counter 0))
            (define (is-nth n)
                (set-cdr! counter (+ 1 (cdr counter)))
                (= n (cdr counter)))
            (define fits-vertical (not (< (point-y (area-size middle-area)) 3)))
            (define vertical-table
                (if (< (point-x (area-size middle-area)) 3)
                    (if fits-vertical #t '())
                    (if fits-vertical (random-bool-seeded (seed-with room-seed 8)) #f)))
            ;(mark-room-areas wall-areas '() (tile 'soil))
            (generate-room-with-furniture
                (seed-with room-seed 12)
                wall-areas
                (list
                    (lambda (inside-index outer-side current-area)
                        (if (> (- (area-area current-area) inside-index) 2)
                            (let
                                (
                                    (put-it
                                        (lambda ()
                                            (try-put-furniture
                                                (area-index current-area inside-index)
                                                (list
                                                    'furniture
                                                    'bed
                                                    (side-combine outer-side side-left)))
                                            #t)))
                                (if (or (= outer-side side-left) (= outer-side side-down))
                                    (if (skip-one)
                                        (put-it)
                                        #f)
                                    (put-it)))
                            #f))))
            (generate-room-with-furniture
                (seed-with room-seed 7)
                (list (cons side-up middle-area))
                (filter (lambda (x) (not (null? x))) (list
                    (if (null? vertical-table)
                        '()
                        (lambda (inside-index outer-side current-area)
                            (let
                                (
                                    (pos (area-index current-area inside-index))
                                    (size (area-size current-area)))
                                (let
                                    ((abs-pos (index-to-pos (point-x size) inside-index)))
                                    (if
                                        (if vertical-table
                                            (< (point-y abs-pos) (- (point-y size) 2))
                                            (< (point-x abs-pos) (- (point-x size) 2)))
                                        (begin
                                            (try-put-furniture
                                                (area-index current-area inside-index)
                                                (list
                                                    'furniture
                                                    'wood_chair
                                                    (if vertical-table side-up side-left)))
                                            #t)
                                        #f)))))
                    (if vertical-table
                        (lambda (a b c) (is-nth (- (point-x (area-size middle-area)) 1)))
                        '())
                    (if (null? vertical-table)
                        (lambda (a b c) #t)
                        (lambda (inside-index outer-side current-area)
                            (try-put-furniture
                                (area-index current-area inside-index)
                                (list
                                    'furniture
                                    'wood_table
                                    (if vertical-table side-up side-left)))
                            #t))))))
        (define (generate-bathroom room-seed wall-areas middle-area)
            ;(mark-room-areas wall-areas '() (tile 'asphalt))
            (generate-room-with-furniture
                (seed-with room-seed 5)
                wall-areas
                (list
                    (lambda (inside-index outer-side current-area)
                        (big-put-tile
                            this-chunk
                            (area-index current-area inside-index)
                            (cons
                                'marker
                                (list
                                    (list 'furniture 'sink outer-side '(0.0 0.3 0.0) '(0.0 0.0 0.0))
                                    (list 'furniture 'cabinet outer-side '(0.0 0.0 0.0) '(0.0 -0.3 0.0) '(0.0 -0.3 0.0)))))
                        #t))))
        (define (generate-kitchen room-seed wall-areas middle-area)
            (define skip-tracker (cons 'tracker 0))
            (define (skip-one)
                (if (= (cdr skip-tracker) 0)
                    (begin (set-cdr! skip-tracker 1) #f)
                    #t))
            ;(mark-room-areas wall-areas '() (tile 'grassie))
            (generate-room-with-furniture
                (seed-with room-seed 4)
                wall-areas
                (list
                    (lambda (inside-index outer-side current-area)
                        (if (> (- (area-area current-area) inside-index) 2)
                            (begin
                                (try-put-furniture
                                    (area-index current-area inside-index)
                                    (list
                                        'furniture
                                        'wood_chair
                                        (side-combine
                                            outer-side
                                            (if (or (= outer-side side-left) (= outer-side side-down)) side-right side-left))))
                                #t)
                            #f))
                    (lambda (inside-index outer-side current-area)
                        (let
                            (
                                (put-it
                                    (lambda ()
                                        (try-put-furniture
                                            (area-index current-area inside-index)
                                            (list
                                                'furniture
                                                'wood_table
                                                (side-combine outer-side side-left)))
                                        #t)))
                            (if (or (= outer-side side-left) (= outer-side side-down))
                                (if (skip-one)
                                    (put-it)
                                    #f)
                                (put-it)))))))
;        (define (mark-room-areas wall-areas middle-area)
;            (for-each
;                (lambda (wall-area)
;                    (big-fill-area
;                        this-chunk
;                        (cdr wall-area)
;                        (let
;                            ((s (car wall-area)))
;                            (tile (cond
;                                ((= s side-up) 'grassie)
;                                ((= s side-down) 'asphalt)
;                                ((= s side-left) 'wood)
;                                ((= s side-right) 'soil)
;                                (else (display "bugy")))))))
;                wall-areas)
;            (if (not (null? middle-area))
;                (big-fill-area this-chunk middle-area (tile 'brick-path))))
;        (define (generate-kitchen room-seed wall-areas middle-area) (mark-room-areas wall-areas middle-area))
;        (define (generate-main-room room-seed wall-areas middle-area) (mark-room-areas wall-areas middle-area))
;        (define (generate-bathroom room-seed wall-areas middle-area) (mark-room-areas wall-areas middle-area))
        (define (generate-side-room is-right)
            (define (flip-right s)
                (if is-right
                    (if (= s side-left) side-right side-left)
                    s))
            (define (side-offset-pos p)
                (if is-right (make-point (- (- big-size-x 1) (point-x p)) (point-y p)) p))
            (define (side-offset-area a)
                (if is-right
                    (let ((start (side-offset-pos (area-start a))) (size (area-size a)))
                        (make-area
                            (make-point (- (point-x start) (- (point-x size) 1)) (point-y start))
                            size))
                    a))
            (let
                (
                    (variant (random-integer-seeded (seed-with furnitures-seed (if is-right 777 888)) 2))
                    (this-room-seed (seed-with (seed-with furnitures-seed 11111) (if is-right 222 111))))
                (cond
                    ((eq? variant 0)
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 2 4) (make-point 5 1))) wall-tile)
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 2 12) (make-point 7 1))) wall-tile)
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 1 6) (make-point 1 5))) (tile 'glass))
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 1 14) (make-point 1 2))) (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 9)) (single-marker (list 'light 1.6 '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 5 14)) (single-marker (list 'light 1.2)))
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 3)) (single-marker (list 'light 0.7 '(0.0 -0.5 0.0))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point (if (random-bool-seeded this-room-seed) 3 4) 12))
                            (single-marker (list 'door side-right 'metal 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point (+ (random-integer-seeded this-room-seed 3) 3) 4))
                            (single-marker (list 'door side-right 'metal 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 13) (make-point 1 4))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 13) (make-point 1 4))))
                                (cons side-up (side-offset-area (make-area (make-point 5 13) (make-point 3 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 5 1)))))
                            (side-offset-area (make-area (make-point 3 14) (make-point 5 2))))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 2))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 2) (make-point 1 2))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 3 1)))))
                            '())
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 5) (make-point 1 7))))
                                (cons side-up (side-offset-area (make-area (make-point 6 5) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 6) (make-point 1 2))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 9) (make-point 1 3))))
                                (cons side-down (side-offset-area (make-area (make-point 5 11) (make-point 3 1)))))
                            (side-offset-area (make-area (make-point 3 6) (make-point 5 5)))))
                    (else
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 2 7) (make-point 7 1))) wall-tile)
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 2 13) (make-point 4 1))) wall-tile)
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 5 14) (make-point 1 3))) wall-tile)
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 1 9) (make-point 1 3))) (tile 'glass))
                        (big-fill-area this-chunk (side-offset-area (make-area (make-point 3 1) (make-point 3 1))) (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 11)) (single-marker (list 'light 1.4 '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 3 15)) (single-marker (list 'light 0.6)))
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 5)) (single-marker (list 'light 1.0 '(0.0 0.5 0.0))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 5 (if (random-bool-seeded this-room-seed) 15 16)))
                            (single-marker (list 'door side-down 'metal 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point (+ 3 (random-integer-seeded this-room-seed 5)) 7))
                            (single-marker (list 'door side-left 'metal 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 5))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 2) (make-point 1 4))))
                                (cons side-up (side-offset-area (make-area (make-point 7 5) (make-point 1 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 5) (make-point 1 2)))))
                            (side-offset-area (make-area (make-point 3 3) (make-point 3 3))))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 14) (make-point 1 3))))
                                (cons side-up (side-offset-area (make-area (make-point 3 14) (make-point 2 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 1 1)))))
                            (side-offset-area (make-area (make-point 3 15) (make-point 1 1))))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 8) (make-point 1 5))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 9) (make-point 1 7))))
                                (cons side-down (side-offset-area (make-area (make-point 3 12) (make-point 3 1))))
                                (cons side-down (side-offset-area (make-area (make-point 7 16) (make-point 2 1))))
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 6 12) (make-point 1 3)))))
                            (side-offset-area (make-area (make-point 3 9) (make-point 5 3))))))))
        (define (generate-left-room) (generate-side-room #f))
        (define (generate-right-room) (generate-side-room #t))
        (define (generate-bottom-room)
            (let
                (
                    (variant (random-integer-seeded (seed-with furnitures-seed 999) 2))
                    (this-room-seed (seed-with (seed-with furnitures-seed 22222) 333)))
                (cond
                    ((eq? variant 0)
                        (big-fill-area this-chunk (make-area (make-point 7 18) (make-point 1 4)) wall-tile)
                        (big-fill-area this-chunk (make-area (make-point 16 18) (make-point 1 4)) wall-tile)
                        (big-fill-area this-chunk (make-area (make-point 9 22) (make-point 6 1)) (tile 'glass))
                        (big-put-tile this-chunk (make-point 7 (if (random-bool) 19 20)) (single-marker (list 'door side-down 'metal 1)))
                        (big-put-tile this-chunk (make-point 16 (if (random-bool) 19 20)) (single-marker (list 'door side-down 'metal 1)))
                        (big-put-tile this-chunk (make-point 11 19) (single-marker (list 'light 1.0 '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (make-point 4 20) (single-marker (list 'light 0.8 '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (make-point 19 20) (single-marker (list 'light 0.8 '(0.5 0.0 0.0))))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons side-down (make-area (make-point 8 21) (make-point 8 1)))
                                (cons side-up (make-area (make-point 8 18) (make-point 2 1)))
                                (cons side-up (make-area (make-point 14 18) (make-point 2 1)))
                                (cons side-up (make-area (make-point 10 17) (make-point 1 1)))
                                (cons side-up (make-area (make-point 13 17) (make-point 1 1))))
                            (make-area (make-point 9 19) (make-point 6 2)))
                        (let (
                            (room-calls
                                (if (random-bool-seeded this-room-seed)
                                    (cons generate-bathroom generate-kitchen)
                                    (cons generate-kitchen generate-bathroom))))
                            (begin
                                ((car room-calls)
                                    this-room-seed
                                    (list
                                        (cons side-up (make-area (make-point 2 18) (make-point 5 1)))
                                        (cons side-down (make-area (make-point 2 21) (make-point 5 1)))
                                        (cons side-left (make-area (make-point 2 19) (make-point 1 2))))
                                    (make-area (make-point 3 19) (make-point 3 2)))
                                ((cdr room-calls)
                                    this-room-seed
                                    (list
                                        (cons side-up (make-area (make-point 17 18) (make-point 5 1)))
                                        (cons side-down (make-area (make-point 17 21) (make-point 5 1)))
                                        (cons side-right (make-area (make-point 21 19) (make-point 1 2))))
                                    (make-area (make-point 18 19) (make-point 3 2))))))
                    (else
                        (big-fill-area this-chunk (make-area (make-point 9 18) (make-point 1 3)) wall-tile)
                        (big-fill-area this-chunk (make-area (make-point 10 19) (make-point 6 1)) wall-tile)
                        (big-fill-area this-chunk (make-area (make-point 18 22) (make-point 2 1)) (tile 'glass))
                        (big-fill-area this-chunk (make-area (make-point 11 22) (make-point 3 1)) (tile 'glass))
                        (big-put-tile this-chunk (make-point 11 17) (single-marker (list 'light 0.7 '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (make-point 12 21) (single-marker (list 'light 0.7)))
                        (big-put-tile this-chunk (make-point 5 19) (single-marker (list 'light 1.0)))
                        (big-put-tile this-chunk (make-point 18 19) (single-marker (list 'light 1.0 '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (make-point 15 18) (single-marker (list 'door side-down 'metal 1)))
                        (big-put-tile this-chunk (make-point 9 21) (single-marker (list 'door side-down 'metal 1)))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons side-up (make-area (make-point 17 18) (make-point 4 1)))
                                (cons side-right (make-area (make-point 21 18) (make-point 1 4)))
                                (cons side-down (make-area (make-point 16 21) (make-point 5 1))))
                            (make-area (make-point 16 19) (make-point 5 2)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons side-up (make-area (make-point 10 20) (make-point 6 1)))
                                (cons side-down (make-area (make-point 11 21) (make-point 5 1))))
                            '())
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons side-left (make-area (make-point 2 18) (make-point 1 4)))
                                (cons side-right (make-area (make-point 8 18) (make-point 1 3)))
                                (cons side-down (make-area (make-point 3 21) (make-point 5 1)))
                                (cons side-up (make-area (make-point 3 18) (make-point 5 1))))
                            (make-area (make-point 3 19) (make-point 5 2)))))))
        (put-outer-walls this-chunk)
        (big-fill-area this-chunk (make-area (make-point 7 2) (make-point 1 3)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 16 2) (make-point 1 3)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 2 17) (make-point 8 1)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 14 17) (make-point 8 1)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 9 4) (make-point 1 13)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 14 4) (make-point 1 13)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 10 16) (make-point 4 1)) wall-tile)
        (if (= height 1)
            (begin
                (big-put-tile this-chunk (make-point 11 0) (tile 'air))
                (big-put-tile this-chunk (make-point 12 0) (tile 'air))
                (big-put-tile this-chunk (make-point 11 0) (single-marker (list 'door side-left 'metal 2)))))
        (big-put-tile this-chunk (make-point 8 4) (tile 'concrete))
        (big-put-tile this-chunk (make-point 15 4) (tile 'concrete))
        (big-put-tile this-chunk (make-point 11 5) (single-marker (list 'light 1.5 '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 11 11) (single-marker (list 'light 1.5 '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 9 8) (single-marker (list 'door side-down 'metal 1)))
        (big-put-tile this-chunk (make-point 14 8) (single-marker (list 'door side-down 'metal 1)))
        (big-put-tile this-chunk (make-point (if (random-bool) 11 12) 16) (single-marker (list 'door side-right 'metal 1)))
        (let ((x (if (= (remainder height 4) 3) 9 14)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-up rotation)))
        (cond
            ((eq? part 'tr) (generate-right-room))
            ((eq? part 'r) (generate-right-room))
            ((eq? part 'tl) (generate-left-room))
            ((eq? part 'l) (generate-left-room))
            ((eq? part 't) (generate-left-room) (generate-right-room))
            ((eq? part 'm) (generate-left-room) (generate-right-room))
            ((eq? part 'bl) (generate-left-room) (generate-bottom-room))
            ((eq? part 'b) (generate-bottom-room) (generate-left-room) (generate-right-room))
            ((eq? part 'br) (generate-right-room) (generate-bottom-room)))
        this-chunk)))

))

)
